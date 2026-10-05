//! Host-private certified Media endpoints over the existing Statement Store.
//! Runtime keys and replay admission survive transport reconnects, not runtimes.

use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use futures::stream::BoxStream;
use futures::task::AtomicWaker;
use futures::{FutureExt, Stream, StreamExt, pin_mut, select_biased};
use parity_scale_codec::{Compact, Decode, Encode};
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use truapi::{CallContext, CancellationReason, CancellationToken};
use zeroize::Zeroizing;

use crate::host_logic::media_protocol::{
    EffectiveClock, MAX_ADVERTISEMENT_BYTES, MAX_ADVERTISEMENT_LIFETIME, MAX_PACKET_BYTES,
    MAX_PACKET_LIFETIME, MediaIdentity, MediaProtocolError, RuntimeSecrets,
    VerifiedAdvertisement, advertisement_topic, decode_advertisement, inbox_topic, open_packet,
    seal_packet,
};
use crate::host_logic::statement_store::{
    NewStatements, decode_signed_statement, decode_verified_statement_data,
    parse_new_statements_result,
};
use crate::unix_time::current_unix_secs;
use super::{AuthoritySession, ProductAuthority, RuntimeServices, statement_store_rpc};

const CALL_BUDGET: Duration = Duration::from_secs(5);
const RECONNECT_BUDGET: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(1);
const RENEW_AFTER: u64 = 300;
const EVENT_CAPACITY: usize = 128;
const REPLAY_CAPACITY: usize = 16_384;
const ENDPOINT_CAPACITY: usize = 32;
const PAGE_CAPACITY: usize = 256;
const PAGE_BYTES: usize = 4 * 1024 * 1024;
const LOOKUP_PAGE_CAPACITY: usize = 128;
const LOOKUP_STATEMENT_CAPACITY: usize = 4096;
// The four outer fields are proof, expiry, the full topic, and opaque data.
const STATEMENT_OVERHEAD: usize = 512;

type Result<T, E = MediaSignalingError> = std::result::Result<T, E>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum MediaSignalingError {
    #[error("media authority is not connected")]
    NotConnected,
    #[error("media signaling is closed")]
    Closed,
    #[error("invalid media peer")]
    InvalidPeer,
    #[error("media signaling timed out")]
    TimedOut,
    #[error("media signaling capacity exhausted")]
    Overflow,
    #[error("media signaling unavailable")]
    Unavailable,
}

// Deliberately neither Debug nor Clone: plaintext is private and moved once.
pub(super) struct AuthenticatedMediaMessage {
    pub sender: VerifiedAdvertisement,
    pub expires_at: u64,
    pub plaintext: Zeroizing<Vec<u8>>,
}

pub(super) enum MediaSignalingEvent {
    Message(Box<AuthenticatedMediaMessage>),
    Reconnecting,
    Ready,
}

pub(super) struct MediaSignaling {
    inner: Arc<Inner>,
}

#[derive(Clone)]
struct Connection {
    rpc: RpcClient,
    failed: CancellationToken,
}

struct Live {
    secrets: RuntimeSecrets,
    advertisement: Option<VerifiedAdvertisement>,
    connection: Option<Connection>,
    clock: EffectiveClock,
    // Only certificate renewals; never redirect an established endpoint/key.
    recipient_certificates: Vec<VerifiedAdvertisement>,
}

struct State {
    live: Option<Live>,
    events: VecDeque<MediaSignalingEvent>,
    terminal: Option<MediaSignalingError>,
}

struct Inner {
    services: Arc<RuntimeServices>,
    authority: Arc<dyn ProductAuthority>,
    session: AuthoritySession,
    identity: MediaIdentity,
    origin: Instant,
    closed: CancellationToken,
    state: Mutex<State>,
    event_waker: AtomicWaker,
    // A lookup holds a temporary subscription and at most 32 certificates.
    // Serialize snapshots within each caller's cancellation/deadline budget.
    lookup_gate: futures::lock::Mutex<()>,
}

impl MediaSignaling {
    pub(super) async fn start(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        session: AuthoritySession,
        identity: MediaIdentity,
        cx: &CallContext,
    ) -> Result<(Arc<Self>, BoxStream<'static, Result<MediaSignalingEvent>>)> {
        if identity.network != services.statement_store.genesis_hash() {
            return Err(MediaSignalingError::InvalidPeer);
        }
        advertisement_topic(&identity).map_err(|_| MediaSignalingError::InvalidPeer)?;
        let secrets = RuntimeSecrets::generate(identity.clone(), REPLAY_CAPACITY)
            .map_err(|_| MediaSignalingError::Unavailable)?;
        let inner = Inner::new(services, authority, session, identity, secrets);
        // This owner closes even if startup's future is dropped at any await.
        let owner = Arc::new(Self { inner: inner.clone() });
        let changes = inner.authority.session_state().subscribe();
        inner.ensure_current()?;
        let watcher = inner.clone();
        (inner.services.spawner)(Box::pin(async move {
            watcher.watch_authority(changes).await;
        }));
        let (connection, subscription) = inner
            .bounded(cx, CALL_BUDGET, |operation| async {
                let advertisement = inner.certify(operation).await?;
                let connection = inner.connect().await?;
                let topic = inbox_topic(&inner.identity, &advertisement.advertisement().fields.endpoint_id)
                    .map_err(|_| MediaSignalingError::Unavailable)?;
                // No advertisement is visible before its receiving inbox exists.
                let subscription = statement_store_rpc::subscribe_match_all(&connection.rpc, &[topic])
                    .await
                    .map_err(|_| MediaSignalingError::Unavailable)?;
                inner.publish(&connection, &advertisement).await?;
                inner.install(&connection, advertisement)?;
                Ok((connection, subscription))
            })
            .await?;
        inner.emit(MediaSignalingEvent::Ready)?;
        let driver = inner.clone();
        (inner.services.spawner)(Box::pin(async move {
            let result = driver.drive(connection, subscription).await;
            driver.terminate(result.err().unwrap_or(MediaSignalingError::Closed));
        }));
        let events = Box::pin(EventStream { inner, ended: false });
        Ok((owner, events))
    }

    /// Connected endpoint without a transport, for service tests that
    /// deliver already-authenticated messages directly.
    #[cfg(test)]
    pub(super) fn connected_for_test(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        session: AuthoritySession,
        identity: MediaIdentity,
        advertisement: VerifiedAdvertisement,
    ) -> (Arc<Self>, BoxStream<'static, Result<MediaSignalingEvent>>) {
        let secrets = RuntimeSecrets::generate(identity.clone(), REPLAY_CAPACITY).expect("test secrets generate");
        let inner = Inner::new(services, authority, session, identity, secrets);
        inner.state.lock().expect("media signaling state poisoned").live.as_mut()
            .expect("new endpoint is live").advertisement = Some(advertisement);
        let owner = Arc::new(Self { inner: inner.clone() });
        (owner, Box::pin(EventStream { inner, ended: false }))
    }

    #[cfg(test)]
    pub(super) fn deliver_for_test(&self, message: AuthenticatedMediaMessage) -> Result<()> {
        self.inner.emit(MediaSignalingEvent::Message(Box::new(message)))
    }

    pub(super) fn advertisement(&self) -> Result<VerifiedAdvertisement> {
        self.inner.with_live(|live, now| {
            let advertisement = live.advertisement.as_ref().ok_or(MediaSignalingError::NotConnected)?;
            if advertisement.advertisement().fields.expires_at <= now {
                return Err(MediaSignalingError::TimedOut);
            }
            Ok(advertisement.clone())
        })
    }

    pub(super) fn now(&self) -> Result<u64> {
        self.inner.with_live(|_, now| Ok(now))
    }

    pub(super) async fn lookup(&self, peer: &MediaIdentity, cx: &CallContext) -> Result<Vec<VerifiedAdvertisement>> {
        self.inner.bounded(cx, CALL_BUDGET, |_| async {
            let _guard = self.inner.lookup_gate.lock().await;
            self.lookup_snapshot(peer).await
        }).await
    }

    // The caller owns lookup_gate and its single encompassing deadline.
    async fn lookup_snapshot(&self, peer: &MediaIdentity) -> Result<Vec<VerifiedAdvertisement>> {
        if peer.network != self.inner.identity.network || peer.product_id != self.inner.identity.product_id {
            return Err(MediaSignalingError::InvalidPeer);
        }
        let topic = advertisement_topic(peer).map_err(|_| MediaSignalingError::InvalidPeer)?;
            let (connection, own_endpoint) = self.inner.with_live(|live, _| {
                Ok((
                    live.connection.clone().ok_or(MediaSignalingError::NotConnected)?,
                    live.advertisement.as_ref().ok_or(MediaSignalingError::NotConnected)?
                        .advertisement().fields.endpoint_id,
                ))
            })?;
            // If cancellation wins before the subscription ID arrives, its
            // remote subscription cannot be individually stopped. Retire this
            // RPC epoch instead of leaking unknown lookup subscriptions.
            let mut opening = FailConnectionOnDrop(Some(connection.failed.clone()));
            let mut subscription = statement_store_rpc::subscribe_match_all(&connection.rpc, &[topic])
                .await.map_err(|_| MediaSignalingError::Unavailable)?;
            opening.0.take();
            let mut endpoints: BTreeMap<[u8; 32], VerifiedAdvertisement> = BTreeMap::new();
            let mut count = 0usize;
            for _ in 0..LOOKUP_PAGE_CAPACITY {
                let value = subscription.next().await
                    .ok_or(MediaSignalingError::Unavailable)?
                    .map_err(|_| MediaSignalingError::Unavailable)?;
                let Some(page) = bounded_page(value, MAX_ADVERTISEMENT_BYTES)? else { continue };
                count = count.checked_add(page.statements.len()).ok_or(MediaSignalingError::Overflow)?;
                if count > LOOKUP_STATEMENT_CAPACITY {
                    return Err(MediaSignalingError::Overflow);
                }
                let now = self.now()?;
                for statement in page.statements {
                    let Some((data, expiry)) = verified_payload(&statement, &topic, MAX_ADVERTISEMENT_BYTES, now) else { continue };
                    let Ok(advertisement) = decode_advertisement(&data, peer, now) else { continue };
                    let fields = &advertisement.advertisement().fields;
                    if fields.endpoint_id == own_endpoint || fields.expires_at <= now || expiry != fields.expires_at {
                        continue;
                    }
                    if let Some(old) = endpoints.get(&fields.endpoint_id) {
                        if old.advertisement().fields.issued_at >= fields.issued_at {
                            continue;
                        }
                    } else if endpoints.len() >= ENDPOINT_CAPACITY {
                        return Err(MediaSignalingError::Overflow);
                    }
                    endpoints.insert(fields.endpoint_id, advertisement);
                }
                // Remaining pages are the initial snapshot, not a live lookup.
                // Drop unsubscribes even on cancellation, overflow, or timeout.
                if page.remaining.unwrap_or(0) == 0 {
                    let now = self.now()?;
                    endpoints.retain(|_, advertisement| advertisement.advertisement().fields.expires_at > now);
                    return Ok(endpoints.into_values().collect());
                }
            }
            Err(MediaSignalingError::Overflow)
    }

    pub(super) async fn send(&self, recipient: &VerifiedAdvertisement, plaintext: &[u8], cx: &CallContext) -> Result<()> {
        self.inner.bounded(cx, CALL_BUDGET, |_| async {
            let renewed;
            let recipient = if recipient.advertisement().fields.expires_at.saturating_sub(self.now()?) <= MAX_PACKET_LIFETIME {
                // The outer send deadline also covers gate wait, discovery and
                // final submit. There is no nested fresh five-second budget.
                renewed = self.renewed_recipient(recipient).await?;
                &renewed
            } else {
                recipient
            };
            let (connection, packet, expiry, topic) = self.inner.with_live(|live, now| {
                let sender = live.advertisement.as_ref().ok_or(MediaSignalingError::NotConnected)?;
                let expiry = now.checked_add(MAX_PACKET_LIFETIME).ok_or(MediaSignalingError::Unavailable)?
                    .min(sender.advertisement().fields.expires_at)
                    .min(recipient.advertisement().fields.expires_at);
                let packet = seal_packet(&live.secrets, sender, recipient, plaintext, now, expiry)
                    .map_err(|_| MediaSignalingError::InvalidPeer)?;
                let topic = inbox_topic(&recipient.identity(), &recipient.advertisement().fields.endpoint_id)
                    .map_err(|_| MediaSignalingError::InvalidPeer)?;
                Ok((live.connection.clone().ok_or(MediaSignalingError::NotConnected)?, packet, expiry, topic))
            })?;
            self.inner.submit(&connection, packet, topic, expiry).await
        }).await
    }

    async fn renewed_recipient(&self, previous: &VerifiedAdvertisement) -> Result<VerifiedAdvertisement> {
        let _guard = self.inner.lookup_gate.lock().await;
        // Recheck after acquiring the gate: concurrent sends to this endpoint
        // reuse the first completed renewal instead of opening more snapshots.
        let cached = self.inner.with_live(|live, now| {
            live.recipient_certificates.retain(|advertisement| {
                advertisement.advertisement().fields.expires_at.saturating_sub(now) > MAX_PACKET_LIFETIME
            });
            Ok(live.recipient_certificates.iter().find(|advertisement| {
                same_endpoint(previous, advertisement)
                    && advertisement.advertisement().fields.expires_at > previous.advertisement().fields.expires_at
            }).cloned())
        })?;
        if let Some(cached) = cached {
            return Ok(cached);
        }
        let candidates = self.lookup_snapshot(&previous.identity()).await?;
        self.inner.with_live(|live, now| {
            let renewed = candidates.into_iter().find(|advertisement| {
                let fields = &advertisement.advertisement().fields;
                same_endpoint(previous, advertisement)
                    && fields.issued_at >= previous.advertisement().fields.issued_at
                    && fields.expires_at > previous.advertisement().fields.expires_at
                    && fields.expires_at.saturating_sub(now) > MAX_PACKET_LIFETIME
            }).ok_or(MediaSignalingError::NotConnected)?;
            if let Some(cached) = live.recipient_certificates.iter_mut()
                .find(|advertisement| same_endpoint(&renewed, advertisement))
            {
                *cached = renewed.clone();
            } else if live.recipient_certificates.len() < ENDPOINT_CAPACITY {
                live.recipient_certificates.push(renewed.clone());
            }
            Ok(renewed)
        })
    }

    pub(super) fn close(&self) {
        self.inner.terminate(MediaSignalingError::Closed);
    }
}

impl Drop for MediaSignaling {
    fn drop(&mut self) {
        self.close();
    }
}

impl Inner {
    fn new(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        session: AuthoritySession,
        identity: MediaIdentity,
        secrets: RuntimeSecrets,
    ) -> Arc<Self> {
        Arc::new(Self {
            services,
            authority,
            session,
            identity,
            origin: Instant::now(),
            closed: CancellationToken::default(),
            state: Mutex::new(State {
                live: Some(Live {
                    secrets,
                    advertisement: None,
                    connection: None,
                    clock: EffectiveClock::new(current_unix_secs(), Duration::ZERO),
                    recipient_certificates: Vec::new(),
                }),
                events: VecDeque::with_capacity(EVENT_CAPACITY),
                terminal: None,
            }),
            event_waker: AtomicWaker::new(),
            lookup_gate: futures::lock::Mutex::new(()),
        })
    }

    fn ensure_current(&self) -> Result<()> {
        if let Some(error) = self.state.lock().expect("media signaling state poisoned").terminal {
            return Err(error);
        }
        let valid = self.authority.current_session().is_some_and(|current| {
            current.validation_id == self.session.validation_id
                && current.public_key == self.session.public_key
                && current.identity_account_id == self.session.identity_account_id
        });
        if !valid {
            self.terminate(MediaSignalingError::NotConnected);
            return Err(MediaSignalingError::NotConnected);
        }
        Ok(())
    }

    fn terminal(&self) -> MediaSignalingError {
        self.state.lock().expect("media signaling state poisoned").terminal
            .unwrap_or(MediaSignalingError::Closed)
    }

    fn with_live<T>(&self, action: impl FnOnce(&mut Live, u64) -> Result<T>) -> Result<T> {
        self.ensure_current()?;
        let mut state = self.state.lock().expect("media signaling state poisoned");
        let error = state.terminal.unwrap_or(MediaSignalingError::Closed);
        let live = state.live.as_mut().ok_or(error)?;
        let now = live.clock.now(current_unix_secs(), self.origin.elapsed())
            .map_err(|_| MediaSignalingError::Unavailable)?;
        action(live, now)
    }

    fn terminate(&self, error: MediaSignalingError) {
        let released = {
            let mut state = self.state.lock().expect("media signaling state poisoned");
            if state.terminal.is_some() {
                return;
            }
            // Fence first. Drop RPC handles outside the lock: their cleanup
            // can invoke executor wakers, including an inline host executor.
            state.terminal = Some(error);
            (state.live.take(), std::mem::take(&mut state.events))
        };
        self.closed.cancel();
        drop(released);
        self.event_waker.wake();
    }

    fn emit(&self, event: MediaSignalingEvent) -> Result<()> {
        self.ensure_current()?;
        let mut state = self.state.lock().expect("media signaling state poisoned");
        if let Some(error) = state.terminal {
            return Err(error);
        }
        if state.events.len() >= EVENT_CAPACITY {
            drop(state);
            self.terminate(MediaSignalingError::Overflow);
            return Err(MediaSignalingError::Overflow);
        }
        state.events.push_back(event);
        drop(state);
        self.event_waker.wake();
        Ok(())
    }

    async fn bounded<T, F, Fut>(&self, caller: &CallContext, budget: Duration, action: F) -> Result<T>
    where
        F: FnOnce(CallContext) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        self.ensure_current()?;
        let duration = caller.timeout().unwrap_or(budget).min(budget);
        let cancellation = CancellationToken::default();
        let _cancel_on_drop = CancelOnDrop(cancellation.clone());
        let mut operation = CallContext::with_parts(caller.request_id().to_owned(), cancellation);
        operation.set_timeout(duration);
        let call = action(operation).fuse();
        let cancelled = caller.cancel().cancelled().fuse();
        let closed = self.closed.cancelled().fuse();
        let deadline = futures_timer::Delay::new(duration).fuse();
        pin_mut!(call, cancelled, closed, deadline);
        let result = select_biased! {
            _ = closed => Err(self.terminal()),
            reason = cancelled => Err(match reason {
                CancellationReason::TimedOut { .. } => MediaSignalingError::TimedOut,
                _ => MediaSignalingError::Closed,
            }),
            _ = deadline => Err(MediaSignalingError::TimedOut),
            result = call => result,
        };
        self.ensure_current()?;
        result
    }

    async fn watch_authority(self: Arc<Self>, mut changes: BoxStream<'static, truapi::versioned::account::HostAccountConnectionStatusSubscribeItem>) {
        loop {
            let changed = changes.next().fuse();
            let closed = self.closed.cancelled().fuse();
            // Signing-host activation generations can change without changing
            // the outward connected/disconnected shape. Check those as well.
            let tick = futures_timer::Delay::new(RETRY_DELAY).fuse();
            pin_mut!(changed, closed, tick);
            select_biased! {
                _ = closed => return,
                event = changed => {
                    if event.is_none() || matches!(event, Some(truapi::versioned::account::HostAccountConnectionStatusSubscribeItem::V1(truapi::v01::HostAccountConnectionStatusSubscribeItem::Disconnected))) {
                        self.terminate(MediaSignalingError::NotConnected);
                        return;
                    }
                },
                _ = tick => {},
            }
            if self.ensure_current().is_err() {
                return;
            }
        }
    }

    async fn certify(&self, cx: CallContext) -> Result<VerifiedAdvertisement> {
        let unsigned = self.with_live(|live, now| {
            let expiry = now.checked_add(MAX_ADVERTISEMENT_LIFETIME).ok_or(MediaSignalingError::Unavailable)?;
            live.secrets.unsigned_advertisement(now, expiry).map_err(|_| MediaSignalingError::Unavailable)
        })?;
        let signature = self.authority.certify_media_endpoint(&cx, &self.session, unsigned.clone())
            .await.map_err(|_| MediaSignalingError::Unavailable)?;
        self.with_live(|_, now| unsigned.authenticate(signature, now).map_err(|_| MediaSignalingError::Unavailable))
    }

    async fn connect(&self) -> Result<Connection> {
        self.ensure_current()?;
        let rpc = self.services.statement_store.client("media-signaling").await
            .map_err(|_| MediaSignalingError::Unavailable)?;
        self.ensure_current()?;
        Ok(Connection { rpc, failed: CancellationToken::default() })
    }

    async fn submit(&self, connection: &Connection, payload: Vec<u8>, topic: [u8; 32], expiry: u64) -> Result<()> {
        self.ensure_current()?;
        if connection.failed.is_cancelled() {
            return Err(MediaSignalingError::NotConnected);
        }
        let statement = self.authority.sign_media_statement(&self.session, payload, vec![topic], expiry)
            .map_err(|_| MediaSignalingError::Unavailable)?;
        self.ensure_current()?;
        // Only new/known acknowledgements succeed; no allowance acquisition,
        // SSO retry helper, fire-and-forget submit, or product permission path.
        // A cancelled in-flight request must not leave a pending request on a
        // reused client forever. Retire that transport, retaining the key epoch.
        let mut pending = FailConnectionOnDrop(Some(connection.failed.clone()));
        if statement_store_rpc::submit(&connection.rpc, statement).await.is_err() {
            connection.failed.cancel();
            return Err(MediaSignalingError::Unavailable);
        }
        pending.0.take();
        self.ensure_current()
    }

    async fn publish(&self, connection: &Connection, advertisement: &VerifiedAdvertisement) -> Result<()> {
        let topic = advertisement_topic(&self.identity).map_err(|_| MediaSignalingError::Unavailable)?;
        self.submit(connection, advertisement.advertisement().encode(), topic, advertisement.advertisement().fields.expires_at).await
    }

    fn install(&self, connection: &Connection, advertisement: VerifiedAdvertisement) -> Result<()> {
        self.with_live(|live, now| {
            if advertisement.advertisement().fields.expires_at <= now {
                return Err(MediaSignalingError::TimedOut);
            }
            live.advertisement = Some(advertisement);
            live.connection = Some(connection.clone());
            Ok(())
        })
    }

    fn receive(&self, value: Value) -> Result<()> {
        let Some(page) = bounded_page(value, MAX_PACKET_BYTES)? else { return Ok(()) };
        for statement in page.statements {
            let message = self.with_live(|live, now| {
                let advertisement = live.advertisement.as_ref().ok_or(MediaSignalingError::NotConnected)?;
                let topic = inbox_topic(&self.identity, &advertisement.advertisement().fields.endpoint_id)
                    .map_err(|_| MediaSignalingError::Unavailable)?;
                let Some((data, expiry)) = verified_payload(&statement, &topic, MAX_PACKET_BYTES, now) else { return Ok(None) };
                let Ok(packet) = open_packet(&live.secrets, advertisement, &data, now) else { return Ok(None) };
                if expiry != packet.expires_at() {
                    return Ok(None);
                }
                match live.secrets.admit_replay(&packet, now) {
                    Ok(()) => {},
                    Err(MediaProtocolError::Replay) => return Ok(None),
                    Err(MediaProtocolError::ReplayFull) => return Err(MediaSignalingError::Overflow),
                    Err(_) => return Err(MediaSignalingError::Unavailable),
                }
                let (sender, _message_id, expires_at, plaintext) = packet.into_parts();
                Ok(Some(AuthenticatedMediaMessage { sender, expires_at, plaintext }))
            })?;
            if let Some(message) = message {
                self.emit(MediaSignalingEvent::Message(Box::new(message)))?;
            }
        }
        Ok(())
    }

    async fn drive(&self, mut connection: Connection, mut subscription: RpcSubscription<Value>) -> Result<()> {
        let mut maintenance = futures_timer::Delay::new(RETRY_DELAY);
        loop {
            let step = {
                let item = subscription.next().fuse();
                let closed = self.closed.cancelled().fuse();
                let failed = connection.failed.cancelled().fuse();
                let tick = (&mut maintenance).fuse();
                pin_mut!(item, closed, failed, tick);
                select_biased! {
                    _ = closed => return Err(self.terminal()),
                    _ = failed => DriverStep::Reconnect,
                    _ = tick => DriverStep::Maintenance,
                    value = item => match value {
                        Some(Ok(value)) => DriverStep::Message(value),
                        Some(Err(_)) | None => DriverStep::Reconnect,
                    },
                }
            };
            let reconnect = match step {
                DriverStep::Message(value) => {
                    self.receive(value)?;
                    false
                },
                DriverStep::Maintenance => {
                    maintenance.reset(RETRY_DELAY);
                    let renew = self.with_live(|live, now| {
                        let advertisement = live.advertisement.as_ref().ok_or(MediaSignalingError::NotConnected)?;
                        Ok(now.saturating_sub(advertisement.advertisement().fields.issued_at) >= RENEW_AFTER)
                    })?;
                    if renew {
                        let result = self.bounded(&CallContext::default(), CALL_BUDGET, |cx| async {
                            let advertisement = self.certify(cx).await?;
                            self.publish(&connection, &advertisement).await?;
                            self.install(&connection, advertisement)
                        }).await;
                        if result.is_err() {
                            self.ensure_current()?;
                            connection.failed.cancel();
                        }
                    }
                    connection.failed.is_cancelled()
                },
                DriverStep::Reconnect => true,
            };
            if reconnect {
                // End the old inbox before opening the replacement. Keys,
                // certificate and replay records remain inside the same Live.
                drop(subscription);
                self.with_live(|live, _| { live.connection = None; Ok(()) })?;
                self.emit(MediaSignalingEvent::Reconnecting)?;
                (connection, subscription) = self.reconnect().await?;
                self.emit(MediaSignalingEvent::Ready)?;
                maintenance.reset(RETRY_DELAY);
            }
        }
    }

    async fn reconnect(&self) -> Result<(Connection, RpcSubscription<Value>)> {
        self.bounded(&CallContext::default(), RECONNECT_BUDGET, |_| async {
            loop {
                let attempt = self.bounded(&CallContext::default(), CALL_BUDGET, |cx| async {
                    let cached = self.with_live(|live, now| {
                        Ok(live.advertisement.as_ref().filter(|advertisement| {
                            now.saturating_sub(advertisement.advertisement().fields.issued_at) < RENEW_AFTER
                                && advertisement.advertisement().fields.expires_at > now
                        }).cloned())
                    })?;
                    let advertisement = match cached {
                        Some(advertisement) => advertisement,
                        None => self.certify(cx).await?,
                    };
                    let connection = self.connect().await?;
                    let topic = inbox_topic(&self.identity, &advertisement.advertisement().fields.endpoint_id)
                        .map_err(|_| MediaSignalingError::Unavailable)?;
                    let subscription = statement_store_rpc::subscribe_match_all(&connection.rpc, &[topic])
                        .await.map_err(|_| MediaSignalingError::Unavailable)?;
                    self.publish(&connection, &advertisement).await?;
                    self.install(&connection, advertisement)?;
                    Ok((connection, subscription))
                }).await;
                if let Ok(connected) = attempt {
                    return Ok(connected);
                }
                self.ensure_current()?;
                futures_timer::Delay::new(RETRY_DELAY).await;
            }
        }).await
    }
}

enum DriverStep {
    Message(Value),
    Maintenance,
    Reconnect,
}

struct FailConnectionOnDrop(Option<CancellationToken>);

impl Drop for FailConnectionOnDrop {
    fn drop(&mut self) {
        if let Some(failed) = self.0.take() {
            failed.cancel();
        }
    }
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct EventStream {
    inner: Arc<Inner>,
    ended: bool,
}

impl Stream for EventStream {
    type Item = Result<MediaSignalingEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.ended {
            return Poll::Ready(None);
        }
        this.inner.event_waker.register(cx.waker());
        // Do not deliver an already queued plaintext after authority loss.
        let _ = this.inner.ensure_current();
        let mut state = this.inner.state.lock().expect("media signaling state poisoned");
        if let Some(error) = state.terminal {
            this.ended = true;
            return Poll::Ready(Some(Err(error)));
        }
        match state.events.pop_front() {
            Some(event) => Poll::Ready(Some(Ok(event))),
            None => Poll::Pending,
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.inner.terminate(MediaSignalingError::Closed);
    }
}

// Bound the existing RPC parser before it decodes hex or allocates statement
// vectors. Invalid individual entries must not suppress valid siblings.
fn bounded_page(mut value: Value, payload_limit: usize) -> Result<Option<NewStatements>> {
    if value.get("event").and_then(Value::as_str) != Some("newStatements") {
        return Ok(None);
    }
    let Some(statements) = value.get_mut("data").and_then(|data| data.get_mut("statements")).and_then(Value::as_array_mut) else {
        return Ok(None);
    };
    if statements.len() > PAGE_CAPACITY {
        return Err(MediaSignalingError::Overflow);
    }
    let mut bytes = 0usize;
    statements.retain(|statement| {
        let Some(encoded) = statement.as_str() else { return false };
        let encoded = encoded.strip_prefix("0x").unwrap_or(encoded);
        if encoded.len() > (payload_limit + STATEMENT_OVERHEAD) * 2
            || encoded.len() % 2 != 0
            || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return false;
        }
        bytes = bytes.saturating_add(encoded.len() / 2);
        true
    });
    if bytes > PAGE_BYTES {
        return Err(MediaSignalingError::Overflow);
    }
    Ok(parse_new_statements_result(String::new(), &value).ok())
}

fn verified_payload(statement: &[u8], topic: &[u8; 32], limit: usize, now: u64) -> Option<(Vec<u8>, u64)> {
    if statement.len() > limit + STATEMENT_OVERHEAD {
        return None;
    }
    // Bound the field vector before the generic decoder sees its compact count.
    // No private framing: both shape and proof use the existing statement codec.
    if Compact::<u32>::decode(&mut &statement[..]).ok()?.0 != 4 {
        return None;
    }
    let shape = decode_signed_statement(statement).ok()?;
    if shape.topics.as_slice() != [*topic] || shape.channel.is_some() || shape.decryption_key.is_some() {
        return None;
    }
    let expiry = shape.expiry?;
    if expiry & u32::MAX as u64 != 0 || expiry >> 32 < now {
        return None;
    }
    if shape.data.as_ref()?.len() > limit {
        return None;
    }
    drop(shape);
    let verified = decode_verified_statement_data(statement, None).ok()?;
    Some((verified.data, expiry >> 32))
}

fn same_endpoint(left: &VerifiedAdvertisement, right: &VerifiedAdvertisement) -> bool {
    let left = &left.advertisement().fields;
    let right = &right.advertisement().fields;
    left.network == right.network
        && left.product_id == right.product_id
        && left.account == right.account
        && left.endpoint_id == right.endpoint_id
        && left.signing_key == right.signing_key
        && left.encryption_key == right.encryption_key
}
