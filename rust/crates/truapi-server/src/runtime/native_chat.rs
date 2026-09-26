//! Non-exportable native Chat crypto and shared main-purse payment custody.
//!
//! A dropped product call never cancels an already-started durable operation.
//! The registry is wallet/network scoped, not product storage, and keeps one
//! writer for each purse and each installation-owned Chat device.

mod actor;
mod background;
mod hop;
mod hop_access;
mod identity;
mod payments;
mod state_pages;
mod store;
mod wallet;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures::{channel::oneshot, lock::Mutex};
use parity_scale_codec::{DecodeAll, Encode};
use truapi::latest::{
    HostProductDeviceChatError as ChatError, HostProductDeviceChatRequest as Request,
    HostProductDeviceChatResponse as Response,
};
use truapi_platform::{CoreStorageKey, normalize_product_identifier};
use zeroize::Zeroizing;

use super::{authority::AuthoritySession, services::RuntimeServices};
use actor::NativeChatActor;
use payments::WalletCoinage;
use wallet::{SelectedWallet, WalletBinding};

/// Authority captured for one product call, rechecked before every new effect.
#[derive(Clone)]
pub(crate) struct NativeChatContext {
    pub(crate) services: Arc<RuntimeServices>,
    pub(crate) session: AuthoritySession,
    pub(crate) entropy: Zeroizing<Vec<u8>>,
    pub(crate) session_valid: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(crate) network_suffix: String,
    pub(crate) genesis_hash: [u8; 32],
    pub(crate) coinage_instance_id: Option<u32>,
}

impl NativeChatContext {
    pub(super) fn require_current(&self) -> Result<(), ChatError> {
        if (self.session_valid)() {
            Ok(())
        } else {
            Err(ChatError::NotConnected)
        }
    }
}

type WalletKey = ([u8; 32], [u8; 32]);
type DeviceKey = (WalletKey, String);

#[derive(Default)]
struct SessionCache {
    // Hold initialization gates across open; owned work retains this cache
    // after release, but cannot repopulate the next session's cache.
    wallets: Mutex<HashMap<WalletKey, Arc<SelectedWallet>>>,
    chats: Mutex<HashMap<DeviceKey, Arc<NativeChatActor>>>,
    state_pages: state_pages::StatePages,
}

#[derive(Default)]
struct RegistryState {
    cache: parking_lot::Mutex<Arc<SessionCache>>,
    recoveries: parking_lot::Mutex<HashMap<WalletKey, background::Recovery>>,
    // Nonsecret uncertainty must survive session cache eviction.
    products: Mutex<HashSet<WalletKey>>,
}

/// Shared authority-owned state, never instantiated per product request.
#[derive(Clone, Default)]
pub(crate) struct NativeChatRegistry {
    state: Arc<RegistryState>,
}

impl NativeChatRegistry {
    /// Release session secrets without cancelling already-owned durable work.
    /// Store ownership excludes a new allocator until that work ends.
    pub(crate) fn release(&self) {
        self.state.recoveries.lock().clear();
        *self.state.cache.lock() = Arc::default();
    }

    fn products_key(context: &NativeChatContext) -> CoreStorageKey {
        CoreStorageKey::NativeChatProducts {
            root_public_key: context.session.public_key,
            genesis_hash: context.genesis_hash,
        }
    }

    async fn load_products(&self, context: &NativeChatContext) -> Result<Vec<String>, ChatError> {
        context.require_current()?;
        let stored = context
            .services
            .platform
            .read_core_storage(Self::products_key(context))
            .await
            .map_err(|_| ChatError::StorageUnavailable)?;
        context.require_current()?;
        let Some(bytes) = stored else {
            return Ok(Vec::new());
        };
        if bytes.len() > 256 * 260 + 8 {
            return Err(ChatError::StorageUnavailable);
        }
        let (version, products) = <(u8, Vec<String>)>::decode_all(&mut bytes.as_slice())
            .map_err(|_| ChatError::StorageUnavailable)?;
        if version != 1
            || products.len() > 256
            || products.windows(2).any(|pair| pair[0] >= pair[1])
            || products
                .iter()
                .any(|product| normalize_product_identifier(product).as_ref() != Ok(product))
        {
            return Err(ChatError::StorageUnavailable);
        }
        Ok(products)
    }

    async fn persist_products(
        &self,
        context: &NativeChatContext,
        products: &[String],
        uncertain: &mut HashSet<WalletKey>,
    ) -> Result<(), ChatError> {
        context.require_current()?;
        let wallet = (context.session.public_key, context.genesis_hash);
        uncertain.insert(wallet);
        context
            .services
            .platform
            .write_core_storage(Self::products_key(context), (1u8, products).encode())
            .await
            .map_err(|_| ChatError::StorageUnavailable)?;
        uncertain.remove(&wallet);
        Ok(())
    }

    /// Forgetting owns its write even if the host administration call is dropped.
    pub(crate) async fn forget_product(
        &self,
        context: &NativeChatContext,
        product: &str,
    ) -> Result<(), ChatError> {
        let registry = self.clone();
        let context = context.clone();
        let product = product.to_owned();
        let spawner = context.services.spawner.clone();
        let (send, receive) = oneshot::channel();
        spawner(Box::pin(async move {
            // Product revocation does not abandon accepted wallet custody.
            registry.resume_wallet_recovery(context.clone());
            let result = registry.forget_product_owned(&context, &product).await;
            let _ = send.send(result);
        }));
        receive.await.unwrap_or(Err(ChatError::StorageUnavailable))
    }

    async fn forget_product_owned(
        &self,
        context: &NativeChatContext,
        product: &str,
    ) -> Result<(), ChatError> {
        let mut uncertain = self.state.products.lock().await;
        let cache = self.state.cache.lock().clone();
        let key = (
            (context.session.public_key, context.genesis_hash),
            product.to_owned(),
        );
        cache.chats.lock().await.remove(&key);
        cache.state_pages.forget(&key).await;
        let mut products = self.load_products(context).await?;
        match products.binary_search_by(|value| value.as_str().cmp(product)) {
            Ok(index) => {
                products.remove(index);
            }
            Err(_) if !uncertain.contains(&(context.session.public_key, context.genesis_hash)) => {
                return Ok(());
            }
            Err(_) => {}
        }
        self.persist_products(context, &products, &mut uncertain)
            .await
    }

    /// Generic incoming coin import shares the wallet's allocator and recovery
    /// store, but neither creates a Chat device nor requires Chat permission.
    pub(crate) fn top_up(
        &self,
        context: NativeChatContext,
        product: String,
        request: truapi::v01::HostPaymentTopUpRequest,
    ) -> impl Future<Output = Result<(), truapi::v01::HostPaymentTopUpError>> + Send + '_ {
        // Construct the guard before returning the future, including when the
        // caller cancels it without ever polling wallet initialization.
        let request = crate::host_logic::sso::messages::PaymentTopUpRequest {
            calling_product_id: product,
            payload: truapi::versioned::payment::HostPaymentTopUpRequest::V1(request),
        };
        async move {
            let truapi::versioned::payment::HostPaymentTopUpRequest::V1(payload) = &request.payload;
            if payload
                .into
                .is_some_and(|purse| purse != truapi::v01::MAIN_PURSE)
                || !matches!(
                    &payload.source,
                    truapi::v01::PaymentTopUpSource::Coins { .. }
                )
            {
                return Err(truapi::v01::HostPaymentTopUpError::InvalidSource);
            }
            let wallet = self.wallet(&context).await.map_err(|_| {
                truapi::v01::HostPaymentTopUpError::Unknown {
                    reason: "Wallet custody is unavailable".into(),
                }
            })?;
            // Install recovery before yielding ownership to the claim task:
            // a caller may disappear while that task commits its first memo.
            self.resume_wallet_recovery(context.clone());
            let (product, request) = request.into_parts();
            wallet.top_up(&context, &product, request).await
        }
    }

    pub(crate) async fn execute(
        &self,
        context: NativeChatContext,
        calling_product_id: String,
        request: Request,
    ) -> Result<Response, ChatError> {
        context.require_current()?;
        let owned = self.clone();
        let spawner = context.services.spawner.clone();
        let (send, receive) = oneshot::channel();
        spawner(Box::pin(async move {
            let result = owned
                .execute_owned(context, calling_product_id, request)
                .await;
            let _ = send.send(result);
        }));
        receive.await.unwrap_or(Err(ChatError::StorageUnavailable))
    }

    async fn chat(
        &self,
        context: &NativeChatContext,
        product: &str,
    ) -> Result<Arc<NativeChatActor>, ChatError> {
        let key = (
            (context.session.public_key, context.genesis_hash),
            product.to_owned(),
        );
        let cache = self.state.cache.lock().clone();
        let mut chats = cache.chats.lock().await;
        context.require_current()?;
        if let Some(actor) = chats.get(&key) {
            return Ok(actor.clone());
        }
        if chats.len() >= 256 {
            return Err(ChatError::StorageUnavailable);
        }
        context.require_current()?;
        let opened = NativeChatActor::open(context, product).await?;
        context.require_current()?;
        // Failed opens are retryable; the store owns durable uncertainty.
        chats.insert(key, opened.clone());
        Ok(opened)
    }

    async fn denomination(&self, context: &NativeChatContext) -> Result<u128, ChatError> {
        context.require_current()?;
        if context.services.native_wallet.is_some() {
            self.wallet(context).await?.denomination(context).await
        } else {
            payments::coinage_cents_unit(context).await
        }
    }

    pub(super) async fn wallet(
        &self,
        context: &NativeChatContext,
    ) -> Result<Arc<SelectedWallet>, ChatError> {
        let key = (context.session.public_key, context.genesis_hash);
        let cache = self.state.cache.lock().clone();
        let mut wallets = cache.wallets.lock().await;
        context.require_current()?;
        if let Some(wallet) = wallets.get(&key) {
            wallet.check(context)?;
            return Ok(wallet.clone());
        }
        if wallets.len() >= 16 {
            return Err(ChatError::StorageUnavailable);
        }
        context.require_current()?;
        let wallet = match &context.services.native_wallet {
            Some(native_wallet) => {
                SelectedWallet::native(WalletBinding::new(context, native_wallet.clone()))
            }
            None => SelectedWallet::Rust(WalletCoinage::open(context).await?),
        };
        let wallet = Arc::new(wallet);
        context.require_current()?;
        wallets.insert(key, wallet.clone());
        Ok(wallet)
    }

    /// Only absence of native custody permits probing the guarded Rust store.
    /// Native recovery always reaches the native service, even before Chat opens.
    pub(super) async fn existing_wallet(
        &self,
        context: &NativeChatContext,
        required: bool,
    ) -> Result<Option<Arc<SelectedWallet>>, ChatError> {
        let key = (context.session.public_key, context.genesis_hash);
        context.require_current()?;
        if context.services.native_wallet.is_some() {
            return self.wallet(context).await.map(Some);
        }
        let cache = self.state.cache.lock().clone();
        let mut wallets = cache.wallets.lock().await;
        context.require_current()?;
        if let Some(wallet) = wallets.get(&key) {
            wallet.check(context)?;
            return Ok(Some(wallet.clone()));
        }
        let stored = context
            .services
            .platform
            .read_core_storage(CoreStorageKey::MainPurseCoinage {
                root_public_key: context.session.public_key,
                genesis_hash: context.genesis_hash,
            })
            .await;
        context.require_current()?;
        match stored {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) if !required => return Ok(None),
            Ok(None) | Err(_) => return Err(ChatError::StorageUnavailable),
        }
        if wallets.len() >= 16 {
            return Err(ChatError::StorageUnavailable);
        }
        // Open authenticates before exposing any durable payments. Never replace
        // unreadable existing custody with an empty purse or a successful claim.
        let wallet = Arc::new(SelectedWallet::Rust(
            WalletCoinage::open_existing(context).await?,
        ));
        context.require_current()?;
        wallets.insert(key, wallet.clone());
        Ok(Some(wallet))
    }

    async fn execute_owned(
        &self,
        context: NativeChatContext,
        product: String,
        mut request: Request,
    ) -> Result<Response, ChatError> {
        context.require_current()?;
        let requires_wallet = matches!(
            &request,
            Request::SendPayment { .. }
                | Request::PaymentStatus { .. }
                | Request::ReconcilePayments
                | Request::PaymentDenomination
        );
        let cache = self.state.cache.lock().clone();
        let key = (
            (context.session.public_key, context.genesis_hash),
            product.clone(),
        );
        if let Request::ContinueState { state_id, cursor } = &request {
            let response = cache.state_pages.read(&key, *state_id, *cursor).await?;
            context.require_current()?;
            return Ok(response);
        }
        let chat = self.chat(&context, &product).await?;
        let mut binding = None;
        let mut opened = Vec::new();
        let mut prepared = Vec::new();
        let mut open_page = None;
        let mut coinage_cents_unit = None;
        let operation = async {
            match &mut request {
                Request::Initialize => {
                    chat.drive_files(&context).await?;
                    chat.publish_profile_reference(&context).await?;
                }
                Request::Bind { username } => {
                    binding = Some(chat.bind(&context, std::mem::take(username)).await?);
                }
                Request::Prepare {
                    peer_identity,
                    route,
                    plaintext,
                } => {
                    prepared = chat
                        .prepare(&context, *peer_identity, *route, std::mem::take(plaintext))
                        .await?;
                }
                Request::Open { statement } => {
                    let statement = truapi::latest::SignedStatement {
                        proof: statement.proof.clone(),
                        decryption_key: statement.decryption_key.take(),
                        expiry: statement.expiry.take(),
                        channel: statement.channel.take(),
                        topics: std::mem::take(&mut statement.topics),
                        data: statement.data.take(),
                    };
                    (opened, open_page) = chat.open_statement(&context, self, statement).await?;
                }
                Request::ContinueOpen { open_id, cursor } => {
                    (opened, open_page) = chat.continue_open(&context, *open_id, *cursor).await?;
                }
                Request::PrepareAttachments {
                    peer_identity,
                    request_id,
                    text,
                } => {
                    background::require_upload_authorized(&context, &product).await?;
                    chat.prepare_attachments(
                        &context,
                        *peer_identity,
                        std::mem::take(request_id),
                        text.take(),
                    )
                    .await?;
                }
                Request::OpenAttachment { attachment_id } => {
                    chat.open_attachment(&context, *attachment_id).await?;
                }
                Request::SendPayment {
                    peer_identity,
                    request_id,
                    amount_cents,
                } => {
                    let (intent, transport) = chat
                        .payment(
                            &context,
                            *peer_identity,
                            std::mem::take(request_id),
                            *amount_cents,
                        )
                        .await?;
                    self.wallet(&context)
                        .await?
                        .send(&context, intent, transport)
                        .await?;
                }
                Request::PaymentStatus { operation_id } => {
                    let wallet = self.wallet(&context).await?;
                    if !wallet
                        .views(&context, &product)
                        .await?
                        .iter()
                        .any(|view| view.operation_id == *operation_id)
                    {
                        return Err(ChatError::OperationNotFound);
                    }
                }
                Request::ReconcilePayments => chat.reconcile(&context, self).await?,
                Request::PaymentDenomination => {
                    coinage_cents_unit = Some(self.denomination(&context).await?);
                }
                Request::CommitMigration { migration_id } => {
                    chat.commit_migration(&context, *migration_id).await?;
                }
                Request::ContinueState { .. } => unreachable!("handled before actor dispatch"),
            }
            Ok::<(), ChatError>(())
        }
        .await;
        let wallet = cache
            .wallets
            .lock()
            .await
            .get(&(context.session.public_key, context.genesis_hash))
            .cloned();
        if wallet.is_some() {
            self.resume_wallet_recovery(context.clone());
        }
        operation?;
        context.require_current()?;
        let payments = match wallet {
            Some(wallet) => match wallet.views(&context, &product).await {
                Ok(views) => views,
                Err(ChatError::StorageUnavailable) if !requires_wallet && wallet.is_native() => {
                    Vec::new()
                }
                Err(error) => return Err(error),
            },
            None => Vec::new(),
        };
        let mut response = chat.public_view(&context, payments).await?;
        response.binding = binding;
        response.opened = opened;
        response.prepared.extend(prepared);
        response.open_page = open_page;
        response.coinage_cents_unit = coinage_cents_unit;
        let response = cache.state_pages.start(key, response).await?;
        context.require_current()?;
        Ok(response)
    }
}
