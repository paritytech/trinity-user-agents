//! Host-owned native Chat transport and main-purse payment authority.
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
mod store;

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
    wallets: Mutex<HashMap<WalletKey, Arc<WalletCoinage>>>,
    chats: Mutex<HashMap<DeviceKey, Arc<NativeChatActor>>>,
}

#[derive(Default)]
struct RegistryState {
    cache: parking_lot::Mutex<Arc<SessionCache>>,
    receivers: parking_lot::Mutex<HashMap<DeviceKey, background::Receiver>>,
    // Nonsecret uncertainty must survive session cache eviction.
    products: Mutex<HashSet<WalletKey>>,
}

/// Shared authority-owned state, never instantiated per product request.
#[derive(Clone, Default)]
pub(crate) struct NativeChatRegistry {
    state: Arc<RegistryState>,
}

impl NativeChatRegistry {
    /// Stop network ownership immediately on logout or session replacement.
    /// Already-owned durable commits retain their existing completion semantics.
    pub(crate) fn stop_receiving(&self) {
        self.state.receivers.lock().clear();
    }

    /// Release session secrets without cancelling already-owned durable work.
    /// Store ownership excludes a new allocator until that work ends.
    pub(crate) fn release(&self) {
        self.stop_receiving();
        *self.state.cache.lock() = Arc::default();
    }

    /// Restore only previously initialized, currently authorized products.
    pub(crate) fn resume_receiving(&self, context: NativeChatContext) {
        let registry = self.clone();
        let spawner = context.services.spawner.clone();
        spawner(Box::pin(async move {
            let mut retry = std::time::Duration::from_secs(1);
            loop {
                match registry.restore_receiving(&context).await {
                    Ok(()) | Err(ChatError::NotConnected) => break,
                    Err(error) => {
                        tracing::warn!(?error, "native Chat receiver restoration failed");
                    }
                }
                futures_timer::Delay::new(retry).await;
                retry = (retry * 2).min(background::MAX_RETRY);
                if context.require_current().is_err() {
                    break;
                }
            }
        }));
    }

    async fn restore_receiving(&self, context: &NativeChatContext) -> Result<(), ChatError> {
        let mut uncertain = self.state.products.lock().await;
        let products = self.load_products(context).await?;
        let mut failure = None;
        if uncertain.contains(&(context.session.public_key, context.genesis_hash)) {
            failure = self
                .persist_products(context, &products, &mut uncertain)
                .await
                .err();
        }
        for product in products {
            context.require_current()?;
            // One bad device/grant must not suppress unrelated valid products.
            let restored = async {
                background::require_authorized(context, &product).await?;
                // Restoration must never generate a replacement for a lost device.
                if context
                    .services
                    .platform
                    .read_core_storage(CoreStorageKey::NativeChatDevice {
                        root_public_key: context.session.public_key,
                        genesis_hash: context.genesis_hash,
                        product_id: product.clone(),
                    })
                    .await
                    .map_err(|_| ChatError::StorageUnavailable)?
                    .is_none()
                {
                    return Err(ChatError::StorageUnavailable);
                }
                let actor = self.chat(context, &product).await?;
                self.ensure_receiving(context, &product, actor).await;
                Ok(())
            }
            .await;
            match restored {
                Ok(()) | Err(ChatError::AccessNotGranted) => {}
                Err(ChatError::NotConnected) => return Err(ChatError::NotConnected),
                Err(error) => {
                    tracing::warn!(?error, %product, "native Chat product restoration failed");
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
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

    async fn remember_product(
        &self,
        context: &NativeChatContext,
        product: &str,
    ) -> Result<(), ChatError> {
        let mut uncertain = self.state.products.lock().await;
        let mut products = self.load_products(context).await?;
        match products.binary_search_by(|value| value.as_str().cmp(product)) {
            Ok(_) if !uncertain.contains(&(context.session.public_key, context.genesis_hash)) => {
                return Ok(());
            }
            Ok(_) => {}
            Err(index) => {
                if products.len() >= 256 {
                    return Err(ChatError::StorageUnavailable);
                }
                products.insert(index, product.to_owned());
            }
        }
        self.persist_products(context, &products, &mut uncertain)
            .await
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
            // Rebind unrelated products even if the index operation fails or
            // its caller disappears; no second cold-restoration loop is needed.
            registry.rebind_receiving(&context, &product).await;
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
        self.state.receivers.lock().remove(&(
            (context.session.public_key, context.genesis_hash),
            product.to_owned(),
        ));
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

    pub(super) async fn wallet(
        &self,
        context: &NativeChatContext,
    ) -> Result<Arc<WalletCoinage>, ChatError> {
        let key = (context.session.public_key, context.genesis_hash);
        let cache = self.state.cache.lock().clone();
        let mut wallets = cache.wallets.lock().await;
        context.require_current()?;
        if let Some(wallet) = wallets.get(&key) {
            return Ok(wallet.clone());
        }
        if wallets.len() >= 16 {
            return Err(ChatError::StorageUnavailable);
        }
        context.require_current()?;
        let wallet = WalletCoinage::open(context).await?;
        context.require_current()?;
        wallets.insert(key, wallet.clone());
        Ok(wallet)
    }

    async fn execute_owned(
        &self,
        context: NativeChatContext,
        product: String,
        request: Request,
    ) -> Result<Response, ChatError> {
        context.require_current()?;
        let chat = self.chat(&context, &product).await?;
        self.remember_product(&context, &product).await?;
        context.require_current()?;
        let operation = async {
            match request {
                Request::Initialize => {}
                Request::Invite { username, text } => chat.invite(&context, username, text).await?,
                Request::Receive { statement } => chat.receive(&context, self, statement).await?,
                Request::AcceptInvitation { invitation_id } => {
                    chat.accept(&context, invitation_id).await?;
                }
                Request::RejectInvitation { invitation_id } => {
                    chat.reject(&context, invitation_id).await?
                }
                Request::Send {
                    peer_identity,
                    request_id,
                    messages,
                } => {
                    chat.send(&context, peer_identity, request_id, messages)
                        .await?;
                }
                Request::SendAttachments {
                    peer_identity,
                    request_id,
                    text,
                } => {
                    background::require_upload_authorized(&context, &product).await?;
                    chat.send_attachments(&context, peer_identity, request_id, text)
                        .await?;
                }
                Request::OpenAttachment { attachment_id } => {
                    chat.open_attachment(&context, attachment_id).await?;
                }
                Request::SendPayment {
                    peer_identity,
                    request_id,
                    amount_cents,
                } => {
                    let (intent, transport) = chat
                        .payment(&context, peer_identity, request_id, amount_cents)
                        .await?;
                    self.wallet(&context)
                        .await?
                        .send(&context, intent, transport)
                        .await?;
                    chat.flush(&context).await?;
                }
                Request::PaymentStatus { operation_id } => {
                    let wallet = self.wallet(&context).await?;
                    if !wallet
                        .views(&product)
                        .await?
                        .iter()
                        .any(|view| view.operation_id == operation_id)
                    {
                        return Err(ChatError::OperationNotFound);
                    }
                }
                Request::Reconcile => {
                    chat.reconcile(&context, self).await?;
                }
            }
            Ok::<(), ChatError>(())
        }
        .await;
        // Even a transport error can follow a durable peer/roster mutation.
        // Refresh ownership and topics before returning that error to a guest.
        self.ensure_receiving(&context, &product, chat.clone())
            .await;
        operation?;
        context.require_current()?;
        let cache = self.state.cache.lock().clone();
        let wallet = cache
            .wallets
            .lock()
            .await
            .get(&(context.session.public_key, context.genesis_hash))
            .cloned();
        let payments = match wallet {
            Some(wallet) => wallet.views(&product).await?,
            None => Vec::new(),
        };
        chat.public_view(&context, payments).await
    }
}
