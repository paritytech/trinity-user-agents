//! Wallet recovery independent of a product connection. Ordinary Chat transport,
//! subscriptions and acknowledgments belong to the product, not this worker.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{
    FutureExt,
    future::{AbortHandle, Abortable},
};
use futures_timer::Delay;
use truapi::latest::{RemotePermission, RemotePermissionRequest};
use truapi_platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};

use super::{ChatError, NativeChatContext, NativeChatRegistry};
use crate::host_logic::permissions::PermissionsService;

const SESSION_INTERVAL: Duration = Duration::from_secs(1);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(30);

pub(super) struct Recovery {
    session: Vec<u8>,
    active: Arc<AtomicBool>,
    abort: AbortHandle,
}

impl Drop for Recovery {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.abort.abort();
    }
}

struct Running(Arc<AtomicBool>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl NativeChatRegistry {
    /// Resume only wallet-owned commitments. This grants neither Chat access
    /// nor authority to prepare another outgoing payment.
    pub(crate) fn resume_wallet_recovery(&self, mut context: NativeChatContext) {
        let key = (context.session.public_key, context.genesis_hash);
        let mut recoveries = self.state.recoveries.lock();
        if context.require_current().is_err() {
            return;
        }
        if recoveries.get(&key).is_some_and(|recovery| {
            recovery.session == context.session.validation_id
                && recovery.active.load(Ordering::Acquire)
        }) {
            return;
        }
        recoveries.remove(&key);
        let (abort, registration) = AbortHandle::new_pair();
        let active = Arc::new(AtomicBool::new(true));
        let session_valid = context.session_valid.clone();
        let recovering = active.clone();
        context.session_valid =
            Arc::new(move || recovering.load(Ordering::Acquire) && session_valid());
        recoveries.insert(
            key,
            Recovery {
                session: context.session.validation_id.clone(),
                active: active.clone(),
                abort,
            },
        );
        drop(recoveries);
        let registry = self.clone();
        let spawner = context.services.spawner.clone();
        spawner(Box::pin(async move {
            let running = Running(active);
            let _ = Abortable::new(
                async {
                    let invalidated = async {
                        loop {
                            Delay::new(SESSION_INTERVAL).await;
                            if context.require_current().is_err() {
                                break;
                            }
                        }
                    }
                    .fuse();
                    let recovery = recover(&registry, &context).fuse();
                    futures::pin_mut!(invalidated, recovery);
                    futures::select! {
                        _ = invalidated => {},
                        _ = recovery => {},
                    }
                },
                registration,
            )
            .await;
            drop(running);
        }));
    }
}

async fn recover(registry: &NativeChatRegistry, context: &NativeChatContext) {
    let mut opened = false;
    let mut delay = RECONCILE_INTERVAL;
    loop {
        if context.require_current().is_err() {
            return;
        }
        let result = async {
            let key = (context.session.public_key, context.genesis_hash);
            let cache = registry.state.cache.lock().clone();
            let wallet = registry.existing_wallet(context, opened).await?;
            let Some(wallet) = wallet else {
                // Serialize empty-worker exit against a concurrent import.
                // Only existing_wallet may decide whether Rust storage is relevant.
                let recoveries = registry.state.recoveries.lock();
                context.require_current()?;
                let wallets = cache
                    .wallets
                    .try_lock()
                    .ok_or(ChatError::StorageUnavailable)?;
                if !wallets.contains_key(&key) {
                    if let Some(recovery) = recoveries.get(&key) {
                        recovery.active.store(false, Ordering::Release);
                    }
                    return Ok(false);
                }
                return Ok(true);
            };
            opened = true;
            wallet.recover(context).await?;
            Ok::<_, ChatError>(true)
        }
        .await;
        match result {
            Ok(false) | Err(ChatError::NotConnected) => return,
            Ok(true) => delay = RECONCILE_INTERVAL,
            Err(_) => delay = (delay * 2).min(MAX_RETRY),
        }
        Delay::new(delay).await;
    }
}

/// Read-only authorization for Host-private Chat crypto and files. Statement
/// submission is authorized separately when the product submits a statement.
pub(super) async fn require_authorized(
    context: &NativeChatContext,
    product: &str,
) -> Result<(), ChatError> {
    context.require_current()?;
    let platform = context.services.platform.as_ref();
    let permissions = PermissionsService::new(platform, platform, product);
    if permissions
        .authorization_status(&PermissionAuthorizationRequest::ChatAuthority)
        .await
        .map_err(|_| ChatError::StorageUnavailable)?
        != PermissionAuthorizationStatus::Authorized
    {
        return Err(ChatError::AccessNotGranted);
    }
    context.require_current()
}

/// Uploads consume the product's separately authorized storage resource.
pub(super) async fn require_upload_authorized(
    context: &NativeChatContext,
    product: &str,
) -> Result<(), ChatError> {
    require_authorized(context, product).await?;
    let platform = context.services.platform.as_ref();
    let permissions = PermissionsService::new(platform, platform, product);
    if permissions
        .authorization_status(&PermissionAuthorizationRequest::Remote(
            RemotePermissionRequest {
                permission: RemotePermission::PreimageSubmit,
            },
        ))
        .await
        .map_err(|_| ChatError::StorageUnavailable)?
        != PermissionAuthorizationStatus::Authorized
    {
        return Err(ChatError::AccessNotGranted);
    }
    context.require_current()
}
