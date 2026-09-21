//! In-process receive ownership, independent of a product connection. This is
//! not an OS background scheduler: suspended or terminated hosts cannot poll.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{
    FutureExt, StreamExt,
    channel::mpsc,
    future::{AbortHandle, Abortable},
    stream::{BoxStream, SelectAll, select_all},
};
use futures_timer::Delay;
use serde_json::Value;
use truapi::latest::{RemotePermission, RemotePermissionRequest};
use truapi_platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};

use super::{ChatError, NativeChatActor, NativeChatContext, NativeChatRegistry};
use crate::{
    host_logic::{
        permissions::PermissionsService,
        statement_store::{
            MAX_MATCH_ANY_TOPICS, TopicFilterKind, decode_signed_statement,
            parse_new_statements_result,
        },
    },
    runtime::statement_store_rpc,
};

const AUTHORIZATION_INTERVAL: Duration = Duration::from_secs(1);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const MAX_RETRY: Duration = Duration::from_secs(30);

pub(super) struct Receiver {
    session: Vec<u8>,
    active: Arc<AtomicBool>,
    abort: AbortHandle,
    wake: mpsc::Sender<()>,
}

impl Drop for Receiver {
    fn drop(&mut self) {
        // Fence effects synchronously, even if the executor has not polled the
        // abort yet. Durable writes already handed off remain independently owned.
        self.active.store(false, Ordering::Release);
        self.abort.abort();
    }
}

struct Running {
    context: NativeChatContext,
    product: String,
    active: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.context.services.worker_ledger.release(&self.product);
    }
}

impl NativeChatRegistry {
    /// Product clearing advances the local activation. Rebind only existing
    /// unrelated receivers from memory, independent of products-index storage.
    pub(super) async fn rebind_receiving(&self, context: &NativeChatContext, forgotten: &str) {
        let wallet = (context.session.public_key, context.genesis_hash);
        let keys: Vec<_> = self
            .state
            .receivers
            .lock()
            .keys()
            .filter(|(owner, product)| *owner == wallet && product != forgotten)
            .cloned()
            .collect();
        let cache = self.state.cache.lock().clone();
        let actors: Vec<_> = {
            let chats = cache.chats.lock().await;
            keys.into_iter()
                .filter_map(|key| chats.get(&key).cloned().map(|actor| (key.1, actor)))
                .collect()
        };
        for (product, actor) in actors {
            self.ensure_receiving(context, &product, actor).await;
        }
    }

    pub(super) async fn ensure_receiving(
        &self,
        context: &NativeChatContext,
        product: &str,
        actor: Arc<NativeChatActor>,
    ) {
        let authorization = require_authorized(context, product).await;
        let key = (
            (context.session.public_key, context.genesis_hash),
            product.to_owned(),
        );
        let mut receivers = self.state.receivers.lock();
        // A late result belonging to a replaced session must not remove its
        // successor's receiver, or insert a task after logout drained the map.
        if context.require_current().is_err() {
            return;
        }
        if matches!(
            authorization,
            Err(ChatError::NotConnected | ChatError::AccessNotGranted)
        ) {
            receivers.remove(&key);
            return;
        }
        if let Some(receiver) = receivers.get_mut(&key) {
            if receiver.session == context.session.validation_id
                && receiver.active.load(Ordering::Acquire)
            {
                let _ = receiver.wake.try_send(());
                return;
            }
        }
        receivers.remove(&key);
        let (abort, registration) = AbortHandle::new_pair();
        let (wake, mut changes) = mpsc::channel(1);
        let active = Arc::new(AtomicBool::new(true));
        let mut context = context.clone();
        let session_valid = context.session_valid.clone();
        let receiving = active.clone();
        context.session_valid =
            Arc::new(move || receiving.load(Ordering::Acquire) && session_valid());
        receivers.insert(
            key,
            Receiver {
                session: context.session.validation_id.clone(),
                active: active.clone(),
                abort,
                wake,
            },
        );
        drop(receivers);
        let registry = self.clone();
        let product = product.to_owned();
        let spawner = context.services.spawner.clone();
        spawner(Box::pin(async move {
            let _ = Abortable::new(
                async move {
                    // Storage unavailability is not revocation. Retain ownership
                    // while paused; authorize again before every new effect.
                    context.services.worker_ledger.acquire(&product);
                    let running = Running {
                        context,
                        product,
                        active,
                    };
                    let mut retry = Duration::from_secs(1);
                    loop {
                        let result = {
                            // Cancel even hung network work on authorization
                            // failure, dropping its streams before waiting.
                            let revoked = async {
                                loop {
                                    Delay::new(AUTHORIZATION_INTERVAL).await;
                                    if let Err(error) =
                                        require_authorized(&running.context, &running.product).await
                                    {
                                        return Err::<(), ChatError>(error);
                                    }
                                }
                            }
                            .fuse();
                            let receive = run(
                                &running.context,
                                &running.product,
                                &actor,
                                &registry,
                                &mut changes,
                            )
                            .fuse();
                            futures::pin_mut!(revoked, receive);
                            futures::select! {
                                result = revoked => result,
                                result = receive => result,
                            }
                        };
                        match result {
                            Err(ChatError::StorageUnavailable) => {
                                Delay::new(retry).await;
                                retry = (retry * 2).min(MAX_RETRY);
                            }
                            _ => break,
                        }
                    }
                },
                registration,
            )
            .await;
        }));
    }
}

/// Read only: Initialize/PaymentStatus must never prompt for submit permission.
pub(super) async fn require_authorized(
    context: &NativeChatContext,
    product: &str,
) -> Result<(), ChatError> {
    context.require_current()?;
    let platform = context.services.platform.as_ref();
    let permissions = PermissionsService::new(platform, platform, product);
    for request in [
        PermissionAuthorizationRequest::ChatAuthority,
        PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
            permission: RemotePermission::StatementSubmit,
        }),
    ] {
        if permissions
            .authorization_status(&request)
            .await
            .map_err(|_| ChatError::StorageUnavailable)?
            != PermissionAuthorizationStatus::Authorized
        {
            return Err(ChatError::AccessNotGranted);
        }
    }
    context.require_current()
}

/// Uploads additionally consume the product's separately authorized storage resource.
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

async fn connect(
    context: &NativeChatContext,
    product: &str,
    actor: &NativeChatActor,
) -> Result<
    (
        SelectAll<BoxStream<'static, Result<Value, ()>>>,
        Vec<[u8; 32]>,
    ),
    ChatError,
> {
    require_authorized(context, product).await?;
    let topics = actor.incoming_topics().await?;
    let rpc = context
        .services
        .statement_store
        .client("native_chat.receive")
        .await
        .map_err(|_| ChatError::NetworkUnavailable)?;
    let mut subscriptions = Vec::new();
    // MatchAny is necessary: root requests, responses, and device requests are
    // distinct routes, not topics required together on one statement.
    for chunk in topics.chunks(MAX_MATCH_ANY_TOPICS) {
        require_authorized(context, product).await?;
        let subscription = statement_store_rpc::subscribe(&rpc, TopicFilterKind::MatchAny, chunk)
            .await
            .map_err(|_| ChatError::NetworkUnavailable)?;
        // SelectAll normally hides an individual stream ending. Treat that as
        // disconnection, or one lost chunk would silently lose incoming routes.
        subscriptions.push(
            subscription
                .map(|item| item.map_err(|_| ()))
                .chain(futures::stream::once(async { Err(()) }))
                .boxed(),
        );
    }
    Ok((select_all(subscriptions), topics))
}

pub(super) async fn receive_notification(
    context: &NativeChatContext,
    product: &str,
    actor: &Arc<NativeChatActor>,
    registry: &NativeChatRegistry,
    notification: Value,
) -> Result<bool, ChatError> {
    let Ok(page) = parse_new_statements_result(String::new(), &notification) else {
        return Ok(false);
    };
    let mut replay_needed = false;
    for bytes in page.statements {
        require_authorized(context, product).await?;
        let Ok(statement) = decode_signed_statement(&bytes) else {
            continue;
        };
        // Authentication, device admission, replay receipts, private claim
        // persistence and claim-before-ACK ordering all remain in the actor.
        match actor.receive(context, registry, statement).await {
            Err(ChatError::NotConnected | ChatError::AccessNotGranted) => {
                return Err(ChatError::NotConnected);
            }
            Err(
                ChatError::StorageUnavailable
                | ChatError::NetworkUnavailable
                | ChatError::AllowanceRequired,
            ) => {
                // Reopen after reconciliation to replay the store's backlog:
                // a failed claim has no receipt yet and must not be forgotten.
                replay_needed = true;
            }
            _ => {}
        }
    }
    Ok(replay_needed)
}

async fn run(
    context: &NativeChatContext,
    product: &str,
    actor: &Arc<NativeChatActor>,
    registry: &NativeChatRegistry,
    changes: &mut mpsc::Receiver<()>,
) -> Result<(), ChatError> {
    let mut retry = Duration::from_secs(1);
    loop {
        require_authorized(context, product).await?;
        let connection = connect(context, product, actor).fuse();
        let timeout = Delay::new(CONNECT_TIMEOUT).fuse();
        futures::pin_mut!(connection, timeout);
        let connected = futures::select! {
            result = connection => result,
            _ = timeout => Err(ChatError::NetworkUnavailable),
        };
        if let Ok((mut subscriptions, topics)) = connected {
            let mut reconcile = Delay::new(RECONCILE_INTERVAL).fuse();
            let mut replay_needed = false;
            let mut refresh = false;
            loop {
                futures::select! {
                    notification = subscriptions.next().fuse() => {
                        let Some(Ok(notification)) = notification else { break };
                        match receive_notification(context, product, actor, registry, notification).await {
                            Ok(replay) => replay_needed |= replay,
                            Err(error) => return Err(error),
                        }
                        retry = Duration::from_secs(1);
                    },
                    change = changes.next().fuse() => {
                        if change.is_none() { return Ok(()); }
                    },
                    _ = reconcile => {
                        require_authorized(context, product).await?;
                        let _ = actor.reconcile(context, registry).await;
                        reconcile = Delay::new(RECONCILE_INTERVAL).fuse();
                        if replay_needed {
                            refresh = true;
                            break;
                        }
                    },
                }
                context.require_current()?;
                if actor
                    .incoming_topics()
                    .await
                    .as_ref()
                    .is_ok_and(|current| *current != topics)
                {
                    refresh = true;
                    break;
                }
            }
            // Dropping the streams unsubscribes before installing the new exact
            // topic set. Replay on reconnect uses the existing durable receipts.
            drop(subscriptions);
            if refresh {
                continue;
            }
        }
        Delay::new(retry).await;
        retry = (retry * 2).min(MAX_RETRY);
    }
}
