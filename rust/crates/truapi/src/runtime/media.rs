//! Host-owned Media: one authority-fenced service per trusted product execution.

mod backend;
mod group;
mod operations;
mod state;

use std::{future::Future, sync::{Arc, Mutex, MutexGuard, Weak}, time::Duration};
use futures::{channel::oneshot, future::BoxFuture, pin_mut, select_biased, stream::BoxStream, FutureExt, StreamExt};
use truapi::{api::Media, v01 as v, versioned::media as wire, CallContext, CallError, CancellationToken, Subscription};
use truapi::versioned::IntoLatest;
use crate::host_logic::media_protocol::MediaIdentity;
use crate::platform::{MediaBackendCommand, MediaBackendResponse, MediaPlatform, PermissionAuthorizationRequest, PermissionStatusHost, Platform, ProductContext, ProductExecutionKind};
use super::{authority::{AuthoritySession, ProductAuthority}, media_identity, media_signaling::{MediaSignaling, MediaSignalingError, MediaSignalingEvent}, services::RuntimeServices, ProductRuntimeHost};
use crate::host_internal::permissions::PermissionsService;
use backend::{end_all_locked, revoke_permission_locked};
use group::GroupEngine;
use state::*;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

/// Bounds one background start: identity resolution plus endpoint certification.
const SIGNALING_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

type SignalingEvents = BoxStream<'static, std::result::Result<MediaSignalingEvent, MediaSignalingError>>;
/// Starts one authenticated signaling endpoint for the current authority.
type Connect = Arc<dyn Fn(&MediaService) -> BoxFuture<'static, Result<SignalingLink>> + Send + Sync>;

struct SignalingLink {
    session: AuthoritySession,
    identity: MediaIdentity,
    signaling: Arc<MediaSignaling>,
    events: SignalingEvents,
}

/// Delay between failed signaling starts: doubles from `min`, capped at `max`.
#[derive(Clone, Copy)]
struct Backoff { min: Duration, max: Duration }
impl Backoff {
    const DEFAULT: Self = Self { min: Duration::from_millis(500), max: Duration::from_secs(10) };
    fn next(self, delay: Duration) -> Duration { delay.saturating_mul(2).clamp(self.min, self.max) }
}

pub(crate) struct MediaService {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    permission_status: Option<Arc<dyn PermissionStatusHost>>,
    authority: Arc<dyn ProductAuthority>,
    product: ProductContext,
    backend: Arc<dyn MediaPlatform>,
    runtime_id: u64,
    state: Mutex<State>,
    init: futures::lock::Mutex<()>,
    closed: CancellationToken,
    weak: Weak<MediaService>,
    connect: Connect,
    backoff: Backoff,
}
// Construct before spawning, so dropping an unpolled task fails closed too.
struct JobGuard { owner: Weak<MediaService>, armed: bool }
impl JobGuard {
    fn new(owner: Weak<MediaService>) -> Self { Self { owner, armed: true } }
    fn complete(&mut self) { self.armed = false; }
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        if self.armed
            && let Some(service) = self.owner.upgrade() { service.close_with_outcome(v::MediaCallOutcome::HostFailed); }
    }
}
impl MediaService {
    fn new(host: &ProductRuntimeHost, backend: Arc<dyn MediaPlatform>) -> Arc<Self> {
        Self::with_signaling(host, backend, Arc::new(connect_signaling), Backoff::DEFAULT)
    }
    fn with_signaling(host: &ProductRuntimeHost, backend: Arc<dyn MediaPlatform>, connect: Connect, backoff: Backoff) -> Arc<Self> {
        let service = Arc::new_cyclic(|weak| Self {
            services: host.services.clone(), platform: host.platform.clone(), permission_status: host.permission_status.clone(),
            authority: host.authority.clone(), product: host.product.clone(), backend, runtime_id: host.core_instance,
            state: Mutex::new(State::new()), init: futures::lock::Mutex::new(()), closed: CancellationToken::default(), weak: weak.clone(),
            connect, backoff,
        });
        service.services.register_media(&service);
        service
    }
    fn lock(&self) -> MutexGuard<'_, State> { self.state.lock().unwrap_or_else(|error| error.into_inner()) }
    fn spawn(&self, future: BoxFuture<'static, ()>) { (self.services.spawner)(future); }
    pub(crate) fn product_id(&self) -> &str { self.product.product_id.as_str() }
    fn permissions_service(&self) -> PermissionsService<'_, dyn Platform, dyn Platform> {
        PermissionsService::new(self.platform.as_ref(), self.platform.as_ref(), &self.product)
            .with_status_host(self.permission_status.as_deref())
    }
    fn current_authority(&self, state: &State) -> bool {
        !state.closed && !state.fatal && state.binding.as_ref().is_some_and(|binding| {
            self.authority.current_session().as_ref().is_some_and(|session| same_authority(session, &binding.session))
        })
    }
    async fn bounded<T, F>(&self, cancel: &CancellationToken, timeout: Duration, allow_closed: bool, future: F) -> Result<T>
    where F: Future<Output = Result<T>> {
        let work = future.fuse();
        let cancelled = cancel.cancelled().fuse();
        let closed = async {
            if allow_closed { futures::future::pending::<()>().await; }
            self.closed.cancelled().await;
        }.fuse();
        let deadline = futures_timer::Delay::new(timeout).fuse();
        pin_mut!(work, cancelled, closed, deadline);
        select_biased! {
            _ = closed => Err(host_failure()),
            _ = cancelled => Err(domain(v::HostMediaError::OperationCancelled)),
            _ = deadline => Err(domain(v::HostMediaError::TimedOut)),
            result = work => result,
        }
    }
    async fn command(&self, command: MediaBackendCommand, cancel: CancellationToken, timeout: Duration) -> Result<MediaBackendResponse> {
        let cleanup = matches!(&command, MediaBackendCommand::CancelOperation { .. } | MediaBackendCommand::CloseSession { .. }
            | MediaBackendCommand::RemovePeer { .. } | MediaBackendCommand::CloseRuntime);
        self.bounded(&cancel, timeout, cleanup, async {
            let session_id = match &command {
                MediaBackendCommand::SetTracks { session_id, .. } | MediaBackendCommand::CreatePeer { session_id, .. }
                | MediaBackendCommand::ApplyDescription { session_id, .. } | MediaBackendCommand::AddIceCandidate { session_id, .. }
                | MediaBackendCommand::SetSurfaces { session_id, .. } => Some(*session_id),
                _ => None,
            };
            if let Some(session_id) = session_id {
                let opening = self.lock().live_session(&session_id)?.backend_opening.clone();
                if let Some(opening) = opening {
                    opening.await?;
                    let state = self.lock();
                    state.live_session(&session_id)?;
                    if !self.current_authority(&state) { return Err(domain(v::HostMediaError::NotConnected)); }
                }
            }
            match self.backend.media_backend_command(&self.product, self.runtime_id, command).await.map_err(|_| host_failure())? {
                MediaBackendResponse::Rejected { failure } => Err(from_failure(failure)),
                response => Ok(response),
            }
        }).await
    }
    async fn capabilities(&self) -> Result<v::MediaCapabilities> {
        {
            let state = self.lock();
            state.ensure_open()?;
            if let Some(capabilities) = &state.capabilities { return Ok(capabilities.clone()); }
        }
        let cancel = CancellationToken::default();
        self.bounded(&cancel, OPERATION_TIMEOUT, false, async {
            let _guard = self.init.lock().await;
            {
                let state = self.lock();
                state.ensure_open()?;
                if let Some(capabilities) = &state.capabilities { return Ok(capabilities.clone()); }
            }
            let capabilities = self.backend.media_backend_capabilities(&self.product).await.map_err(|_| host_failure())?;
            let remote_limit = capabilities.max_remote_participants.min(PARTICIPANT_BUDGET as u16);
            if !capabilities.supported || capabilities.max_sessions == 0 || remote_limit < 5
                || u32::from(capabilities.max_surfaces_per_session) < 2 * (u32::from(remote_limit) + 1) {
                return Err(CallError::Unsupported);
            }
            let capabilities = v::MediaCapabilities {
                contract_versions: vec![1], network: v::MediaNetwork { genesis_hash: self.services.statement_store.genesis_hash() },
                max_remote_participants: remote_limit, max_sessions: capabilities.max_sessions.min(SESSION_BUDGET as u16),
                max_surfaces_per_session: capabilities.max_surfaces_per_session,
                incoming_lifetime_ms: 60_000, operation_timeout_ms: 5_000, consent_timeout_ms: 60_000, reconnect_timeout_ms: 30_000,
                limits: v::MediaRuntimeLimits { max_issued_sessions: SESSION_BUDGET as u32, max_issued_participants: PARTICIPANT_BUDGET as u32,
                    max_issued_incoming: INCOMING_BUDGET as u32, max_operations: OPERATION_BUDGET as u32,
                    max_pending_incoming: INCOMING_LIMIT as u16, max_subscriptions: LISTENER_LIMIT as u16, event_queue_capacity: EVENT_LIMIT as u16 },
            };
            let mut state = self.lock();
            state.ensure_open()?;
            state.capabilities = Some(capabilities.clone());
            Ok(capabilities)
        }).await
    }
    /// Probe the backend and start runtime observation. Signaling starts in
    /// the background, so subscribers get their snapshot without waiting for it.
    async fn activate(self: &Arc<Self>) -> Result<()> {
        self.capabilities().await?;
        let start_backend = {
            let mut state = self.lock();
            state.ensure_open()?;
            !std::mem::replace(&mut state.backend_started, true)
        };
        if start_backend { self.start_backend_observation(); }
        self.start_signaling();
        Ok(())
    }
    /// Wait, within the caller's bound, for the connector to bind signaling to
    /// the current authority. A waiter skips the connector's pending backoff.
    async fn ensure_ready(self: &Arc<Self>) -> Result<()> {
        self.activate().await?;
        let cancel = CancellationToken::default();
        self.bounded(&cancel, OPERATION_TIMEOUT, false, async {
            loop {
                let ready = {
                    let mut state = self.lock();
                    state.ensure_open()?;
                    if self.current_authority(&state) && state.signaling.is_some() { return Ok(()); }
                    if state.binding.is_some() { None } else {
                        if let Some(wake) = state.signaling_wake.take() { let _ = wake.send(()); }
                        let (ready, bound) = oneshot::channel();
                        state.ready_waiters.retain(|waiter| !waiter.is_canceled());
                        state.ready_waiters.push(ready);
                        Some(bound)
                    }
                };
                // A stale binding is replaced by the connector once its stream ends.
                match ready {
                    None => self.lose_identity(v::MediaCallOutcome::IdentityLost),
                    Some(bound) => { let _ = bound.await; }
                }
            }
        }).await
    }
    /// The sole signaling owner: start, drive until loss, then restart with
    /// bounded backoff until the runtime closes. Each binding is a new epoch.
    fn start_signaling(self: &Arc<Self>) {
        {
            let mut state = self.lock();
            if state.ensure_open().is_err() || std::mem::replace(&mut state.signaling_started, true) { return; }
        }
        let weak = self.weak.clone();
        let closed = self.closed.clone();
        let backoff = self.backoff;
        let mut guard = JobGuard::new(weak.clone());
        self.spawn(async move {
            let mut delay = Duration::ZERO;
            while !closed.is_cancelled() {
                if !delay.is_zero() {
                    let Some(wake) = weak.upgrade().and_then(|service| service.signaling_wake()) else { break; };
                    let stop = closed.cancelled().fuse();
                    let wake = wake.fuse();
                    let tick = futures_timer::Delay::new(delay).fuse();
                    pin_mut!(stop, wake, tick);
                    select_biased! { _ = stop => break, _ = wake => {}, _ = tick => {} }
                }
                let Some(service) = weak.upgrade() else { break; };
                let bound = service.bind_signaling().await;
                drop(service);
                match bound {
                    Ok((epoch, events)) => {
                        Self::drive_signaling(&weak, &closed, epoch, events).await;
                        delay = backoff.min;
                    }
                    Err(error) => {
                        delay = backoff.next(delay);
                        tracing::debug!(?error, ?delay, "media signaling unavailable; retrying");
                    }
                }
            }
            guard.complete();
        }.boxed());
    }
    fn signaling_wake(&self) -> Option<oneshot::Receiver<()>> {
        let mut state = self.lock();
        if state.closed { return None; }
        let (wake, woken) = oneshot::channel();
        state.signaling_wake = Some(wake);
        Some(woken)
    }
    async fn bind_signaling(&self) -> Result<(u64, SignalingEvents)> {
        let cancel = CancellationToken::default();
        let link = self.bounded(&cancel, SIGNALING_ATTEMPT_TIMEOUT, false, (self.connect)(self)).await?;
        let advertisement = link.signaling.advertisement().map_err(signaling_error)?;
        let (epoch, waiters) = {
            let mut state = self.lock();
            state.ensure_open()?;
            if state.binding.is_some() || !self.authority.current_session().as_ref().is_some_and(|current| same_authority(current, &link.session)) {
                link.signaling.close();
                return Err(domain(v::HostMediaError::NotConnected));
            }
            state.binding = Some(Binding { session: link.session, identity: link.identity });
            state.group = Some(GroupEngine::new(advertisement));
            state.signaling = Some(link.signaling);
            (state.epoch, std::mem::take(&mut state.ready_waiters))
        };
        for waiter in waiters { let _ = waiter.send(()); }
        Ok((epoch, link.events))
    }
    async fn drive_signaling(weak: &Weak<Self>, closed: &CancellationToken, epoch: u64, mut events: SignalingEvents) {
        loop {
            let event = events.next().fuse();
            let stop = closed.cancelled().fuse();
            pin_mut!(event, stop);
            let event = select_biased! { _ = stop => break, event = event => event };
            let Some(service) = weak.upgrade() else { break; };
            if service.lock().epoch != epoch { break; }
            match event {
                Some(Ok(event)) => service.signaling_event(epoch, event),
                _ => break,
            }
        }
        if let Some(service) = weak.upgrade()
            && service.lock().epoch == epoch { service.lose_identity(v::MediaCallOutcome::ConnectivityLost); }
    }
    fn start_backend_observation(self: &Arc<Self>) {
        let mut events = self.backend.media_backend_events(&self.product, self.runtime_id);
        let weak = self.weak.clone();
        let closed = self.closed.clone();
        let mut guard = JobGuard::new(weak.clone());
        self.spawn(async move {
            loop {
                let event = events.next().fuse();
                let stop = closed.cancelled().fuse();
                pin_mut!(event, stop);
                let event = select_biased! { _ = stop => break, event = event => event };
                let Some(event) = event else { break; };
                let Some(service) = weak.upgrade() else { break; };
                match event {
                    Ok(event) => service.backend_event(event),
                    Err(_) => { service.close_with_outcome(v::MediaCallOutcome::HostFailed); break; }
                }
            }
            if let Some(service) = weak.upgrade() { service.close_with_outcome(v::MediaCallOutcome::HostFailed); }
            guard.complete();
        }.boxed());
        let weak = self.weak.clone();
        let closed = self.closed.clone();
        let mut guard = JobGuard::new(weak.clone());
        self.spawn(async move {
            loop {
                let stop = closed.cancelled().fuse();
                let tick = futures_timer::Delay::new(Duration::from_millis(200)).fuse();
                pin_mut!(stop, tick);
                select_biased! { _ = stop => break, _ = tick => {} }
                let Some(service) = weak.upgrade() else { break; };
                let (fatal, stale) = {
                    let state = service.lock();
                    (state.fatal, state.binding.is_some() && !service.current_authority(&state))
                };
                if fatal { service.close_with_outcome(v::MediaCallOutcome::HostFailed); break; }
                if stale { service.lose_identity(v::MediaCallOutcome::IdentityLost); }
                service.maintenance();
            }
            guard.complete();
        }.boxed());
    }
    pub(crate) fn close(&self) { self.close_with_outcome(v::MediaCallOutcome::RuntimeClosed); }
    fn close_with_outcome(&self, outcome: v::MediaCallOutcome) {
        let (effects, signaling) = {
            let mut state = self.lock();
            if state.closed { return; }
            state.closed = true;
            let effects = end_all_locked(&mut state, outcome);
            state.binding = None;
            state.group = None;
            let signaling = state.signaling.take();
            state.signaling_wake = None;
            state.ready_waiters.clear();
            state.finish_listeners();
            (effects, signaling)
        };
        self.closed.cancel();
        if let Some(signaling) = signaling { signaling.close(); }
        self.dispatch_effects(effects);
        // The foreign adapter must remain alive even if this is called by Drop.
        let backend = self.backend.clone();
        let product = self.product.clone();
        let runtime_id = self.runtime_id;
        self.spawn(async move {
            let close = backend.media_backend_command(&product, runtime_id, MediaBackendCommand::CloseRuntime).fuse();
            let deadline = futures_timer::Delay::new(OPERATION_TIMEOUT).fuse();
            pin_mut!(close, deadline);
            select_biased! { _ = close => {}, _ = deadline => {} }
        }.boxed());
    }
    fn lose_identity(&self, outcome: v::MediaCallOutcome) {
        let (effects, signaling, exhausted) = {
            let mut state = self.lock();
            if state.closed || state.binding.is_none() { return; }
            let effects = end_all_locked(&mut state, outcome);
            state.binding = None;
            state.group = None;
            let signaling = state.signaling.take();
            let exhausted = match state.epoch.checked_add(1) {
                Some(epoch) => { state.epoch = epoch; false },
                None => { state.fatal = true; true },
            };
            (effects, signaling, exhausted)
        };
        if let Some(signaling) = signaling { signaling.close(); }
        self.dispatch_effects(effects);
        if exhausted { self.close_with_outcome(v::MediaCallOutcome::HostFailed); }
    }
    pub(crate) fn permissions_revoked(&self, request: &PermissionAuthorizationRequest) {
        let effects = {
            let mut state = self.lock();
            let permission = match request {
                PermissionAuthorizationRequest::Calling { network, account } => state.binding.as_ref()
                    .filter(|binding| binding.identity.network == *network && binding.identity.account == *account)
                    .map(|_| crate::platform::MediaRevokedPermission::Calling),
                PermissionAuthorizationRequest::Device(v::HostDevicePermissionRequest::Microphone) =>
                    Some(crate::platform::MediaRevokedPermission::Microphone),
                PermissionAuthorizationRequest::Device(v::HostDevicePermissionRequest::Camera) =>
                    Some(crate::platform::MediaRevokedPermission::Camera),
                _ => None,
            };
            if state.closed { return; }
            let Some(permission) = permission else { return; };
            revoke_permission_locked(&mut state, permission)
        };
        self.dispatch_effects(effects);
    }
}
impl Drop for MediaService { fn drop(&mut self) { self.close(); } }

fn connect_signaling(service: &MediaService) -> BoxFuture<'static, Result<SignalingLink>> {
    let (services, authority, product) = (service.services.clone(), service.authority.clone(), service.product.clone());
    async move {
        let mut cx = CallContext::default();
        cx.set_timeout(OPERATION_TIMEOUT);
        let (session, identity) = media_identity::resolve_identity(services.as_ref(), authority.as_ref(), &product, &cx)
            .await.map_err(|_| domain(v::HostMediaError::NotConnected))?;
        let (signaling, events) = MediaSignaling::start(services, authority, session.clone(), identity.clone(), &cx)
            .await.map_err(signaling_error)?;
        Ok(SignalingLink { session, identity, signaling, events })
    }.boxed()
}

fn signaling_error(error: MediaSignalingError) -> Error {
    domain(match error {
        MediaSignalingError::InvalidPeer => v::HostMediaError::InvalidPeer,
        MediaSignalingError::TimedOut => v::HostMediaError::TimedOut,
        MediaSignalingError::Overflow => v::HostMediaError::EventOverflow,
        _ => v::HostMediaError::NotConnected,
    })
}
fn map_error<E>(error: Error, domain: impl FnOnce(v::HostMediaError) -> E) -> CallError<E> {
    match error {
        CallError::Domain(error) => CallError::Domain(domain(error)),
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        _ => CallError::HostFailure { reason: "media host failure".into() },
    }
}
impl ProductRuntimeHost {
    fn media_service(&self) -> Result<&Arc<MediaService>> {
        if self.media_closed.load(std::sync::atomic::Ordering::Acquire) { return Err(host_failure()); }
        if !matches!(self.product.execution_kind, ProductExecutionKind::App | ProductExecutionKind::Worker) { return Err(CallError::Unsupported); }
        let backend = self.media_platform.as_ref().ok_or(CallError::Unsupported)?;
        let service = self.media.get_or_init(|| MediaService::new(self, backend.clone()));
        if self.media_closed.load(std::sync::atomic::Ordering::Acquire) {
            service.close();
            return Err(host_failure());
        }
        Ok(service)
    }
    pub(crate) fn close_media(&self) {
        self.media_closed.store(true, std::sync::atomic::Ordering::Release);
        if let Some(service) = self.media.get() { service.close(); }
    }
}

#[truapi::async_trait]
impl Media for ProductRuntimeHost {
    async fn get_capabilities(&self, _cx: &CallContext, _request: wire::HostMediaGetCapabilitiesRequest)
        -> std::result::Result<wire::HostMediaGetCapabilitiesResponse, CallError<wire::HostMediaGetCapabilitiesError>> {
        let convert = wire::HostMediaGetCapabilitiesError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.capabilities().await
            .map(wire::HostMediaGetCapabilitiesResponse::V1).map_err(|error| map_error(error, convert))
    }
    async fn session_subscribe(&self, _cx: &CallContext, _request: wire::HostMediaSessionSubscribeRequest)
        -> Subscription<wire::HostMediaSessionSubscribeItem, CallError<wire::HostMediaSessionSubscribeError>> {
        let convert = wire::HostMediaSessionSubscribeError::V1;
        let service = match self.media_service() { Ok(service) => service, Err(error) => return Subscription::interrupted(map_error(error, convert)) };
        // Signaling readiness is not part of the snapshot; offers arrive once it binds.
        if let Err(error) = service.activate().await { return Subscription::interrupted(map_error(error, convert)); }
        let listener = match service.lock().subscribe() { Ok(listener) => listener, Err(error) => return Subscription::interrupted(map_error(error, convert)) };
        Subscription::new(listener.map(move |item| item.map(wire::HostMediaSessionSubscribeItem::V1).map_err(|error| map_error(error, convert))))
    }
    async fn create_session(&self, _cx: &CallContext, request: wire::HostMediaCreateSessionRequest)
        -> std::result::Result<wire::HostMediaCreateSessionResponse, CallError<wire::HostMediaCreateSessionError>> {
        let convert = wire::HostMediaCreateSessionError::V1;
        let result = self.media_service().map_err(|error| map_error(error, convert))?.mutate(Mutation::Create(request.into_latest())).await
            .map_err(|error| map_error(error, convert))?;
        match result { MutationReply::Session(session) => Ok(wire::HostMediaCreateSessionResponse::V1(v::HostMediaCreateSessionResponse { session })), _ => Err(map_error(host_failure(), convert)) }
    }
    async fn add_participant(&self, _cx: &CallContext, request: wire::HostMediaAddParticipantRequest)
        -> std::result::Result<wire::HostMediaAddParticipantResponse, CallError<wire::HostMediaAddParticipantError>> {
        let convert = wire::HostMediaAddParticipantError::V1;
        let result = self.media_service().map_err(|error| map_error(error, convert))?.mutate(Mutation::Add(request.into_latest())).await
            .map_err(|error| map_error(error, convert))?;
        match result { MutationReply::Participant(participant) => Ok(wire::HostMediaAddParticipantResponse::V1(v::HostMediaAddParticipantResponse { participant })), _ => Err(map_error(host_failure(), convert)) }
    }
    async fn respond_incoming(&self, _cx: &CallContext, request: wire::HostMediaRespondIncomingRequest)
        -> std::result::Result<wire::HostMediaRespondIncomingResponse, CallError<wire::HostMediaRespondIncomingError>> {
        let convert = wire::HostMediaRespondIncomingError::V1;
        let service = self.media_service().map_err(|error| map_error(error, convert))?;
        let request = request.into_latest();
        let mutation = match request.decision {
            v::MediaIncomingDecision::Refuse => return service.refuse(request.incoming_id).await.map(wire::HostMediaRespondIncomingResponse::V1).map_err(|error| map_error(error, convert)),
            v::MediaIncomingDecision::AcceptNew { operation_id, tracks } => Mutation::Accept { incoming_id: request.incoming_id, operation_id, session_id: None, tracks: Some(tracks) },
            v::MediaIncomingDecision::AcceptExisting { operation_id, session_id } => Mutation::Accept { incoming_id: request.incoming_id, operation_id, session_id: Some(session_id), tracks: None },
        };
        match service.mutate(mutation).await.map_err(|error| map_error(error, convert))? {
            MutationReply::Incoming(response) => Ok(wire::HostMediaRespondIncomingResponse::V1(response)),
            _ => Err(map_error(host_failure(), convert)),
        }
    }
    async fn remove_participant(&self, _cx: &CallContext, request: wire::HostMediaRemoveParticipantRequest)
        -> std::result::Result<wire::HostMediaRemoveParticipantResponse, CallError<wire::HostMediaRemoveParticipantError>> {
        let convert = wire::HostMediaRemoveParticipantError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.remove(request.into_latest()).await
            .map(|()| wire::HostMediaRemoveParticipantResponse::V1).map_err(|error| map_error(error, convert))
    }
    async fn set_local_tracks(&self, _cx: &CallContext, request: wire::HostMediaSetLocalTracksRequest)
        -> std::result::Result<wire::HostMediaSetLocalTracksResponse, CallError<wire::HostMediaSetLocalTracksError>> {
        let convert = wire::HostMediaSetLocalTracksError::V1;
        match self.media_service().map_err(|error| map_error(error, convert))?.mutate(Mutation::Tracks(request.into_latest())).await.map_err(|error| map_error(error, convert))? {
            MutationReply::Session(session) => Ok(wire::HostMediaSetLocalTracksResponse::V1(v::HostMediaSetLocalTracksResponse { session })),
            _ => Err(map_error(host_failure(), convert)),
        }
    }
    async fn set_surfaces(&self, _cx: &CallContext, request: wire::HostMediaSetSurfacesRequest)
        -> std::result::Result<wire::HostMediaSetSurfacesResponse, CallError<wire::HostMediaSetSurfacesError>> {
        let convert = wire::HostMediaSetSurfacesError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.set_surfaces(request.into_latest()).await
            .map(wire::HostMediaSetSurfacesResponse::V1).map_err(|error| map_error(error, convert))
    }
    async fn end_session(&self, _cx: &CallContext, request: wire::HostMediaEndSessionRequest)
        -> std::result::Result<wire::HostMediaEndSessionResponse, CallError<wire::HostMediaEndSessionError>> {
        let convert = wire::HostMediaEndSessionError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.end_session(request.into_latest()).await
            .map(|()| wire::HostMediaEndSessionResponse::V1).map_err(|error| map_error(error, convert))
    }
    async fn get_operation(&self, _cx: &CallContext, request: wire::HostMediaGetOperationRequest)
        -> std::result::Result<wire::HostMediaGetOperationResponse, CallError<wire::HostMediaGetOperationError>> {
        let convert = wire::HostMediaGetOperationError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.operation(request.into_latest().operation_id).await
            .map(wire::HostMediaGetOperationResponse::V1).map_err(|error| map_error(error, convert))
    }
    async fn cancel_operation(&self, _cx: &CallContext, request: wire::HostMediaCancelOperationRequest)
        -> std::result::Result<wire::HostMediaCancelOperationResponse, CallError<wire::HostMediaCancelOperationError>> {
        let convert = wire::HostMediaCancelOperationError::V1;
        self.media_service().map_err(|error| map_error(error, convert))?.cancel_operation(request.into_latest().operation_id).await
            .map(wire::HostMediaCancelOperationResponse::V1).map_err(|error| map_error(error, convert))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use hpke::{Kem, Serializable, kem::X25519HkdfSha256};
    use schnorrkel::{ExpansionMode, MiniSecretKey};
    use crate::host_logic::media_protocol::{ACCOUNT_SIGNING_CONTEXT, UnsignedAdvertisement, VerifiedAdvertisement};
    use crate::platform::{MediaBackendCapabilities, MediaBackendEvent};
    use crate::runtime::media_signaling::AuthenticatedMediaMessage;
    use crate::test_support::{session_info, stub_platform, test_spawner, wait_until};
    use crate::unix_time::current_unix_secs;
    use group::GroupAction;

    const NETWORK: [u8; 32] = [7; 32];
    const PRODUCT: &str = "vox.dot";
    type Item = std::result::Result<wire::HostMediaSessionSubscribeItem, CallError<wire::HostMediaSessionSubscribeError>>;

    struct Backend;
    #[truapi::async_trait]
    impl MediaPlatform for Backend {
        async fn media_backend_capabilities(&self, _: &ProductContext) -> std::result::Result<MediaBackendCapabilities, v::GenericError> {
            Ok(MediaBackendCapabilities { supported: true, max_sessions: 4, max_remote_participants: 8, max_surfaces_per_session: 18 })
        }
        fn media_backend_events(&self, _: &ProductContext, _: u64) -> BoxStream<'static, std::result::Result<MediaBackendEvent, v::GenericError>> {
            futures::stream::pending().boxed()
        }
        async fn media_backend_command(&self, _: &ProductContext, _: u64, command: MediaBackendCommand) -> std::result::Result<MediaBackendResponse, v::GenericError> {
            Ok(match command {
                MediaBackendCommand::RequestConsent { .. } => MediaBackendResponse::Consent { granted: true },
                _ => MediaBackendResponse::Done,
            })
        }
    }

    fn host(connect: Connect, backoff: Backoff) -> ProductRuntimeHost {
        let (mut host, _) = ProductRuntimeHost::new_pairing_for_tests(stub_platform(), ProductRuntimeHost::compat_host_config(),
            ProductContext::new(PRODUCT.into()).unwrap(), test_spawner());
        host.test_session_state().set_session(session_info());
        let backend: Arc<dyn MediaPlatform> = Arc::new(Backend);
        host.media_platform = Some(backend.clone());
        assert!(host.media.set(MediaService::with_signaling(&host, backend, connect, backoff)).is_ok());
        host
    }

    fn subscribe(host: &ProductRuntimeHost) -> Subscription<wire::HostMediaSessionSubscribeItem, CallError<wire::HostMediaSessionSubscribeError>> {
        futures::executor::block_on(host.session_subscribe(&CallContext::default(), wire::HostMediaSessionSubscribeRequest::V1))
    }

    fn next_within(subscription: &mut Subscription<wire::HostMediaSessionSubscribeItem, CallError<wire::HostMediaSessionSubscribeError>>,
        timeout: Duration) -> Option<Option<Item>> {
        futures::executor::block_on(async {
            let item = subscription.next().fuse();
            let deadline = futures_timer::Delay::new(timeout).fuse();
            pin_mut!(item, deadline);
            select_biased! { item = item => Some(item), _ = deadline => None }
        })
    }

    fn event(item: Option<Option<Item>>) -> v::MediaEvent {
        match item {
            Some(Some(Ok(wire::HostMediaSessionSubscribeItem::V1(event)))) => event,
            Some(Some(Err(error))) => panic!("subscription interrupted: {error:?}"),
            Some(None) => panic!("subscription ended"),
            None => panic!("no Media event in time"),
        }
    }

    fn advertisement(account_seed: u8, endpoint_seed: u8) -> VerifiedAdvertisement {
        let now = current_unix_secs();
        let account = MiniSecretKey::from_bytes(&[account_seed; 32]).unwrap().expand_to_keypair(ExpansionMode::Ed25519);
        let signing = MiniSecretKey::from_bytes(&[endpoint_seed; 32]).unwrap().expand_to_keypair(ExpansionMode::Ed25519);
        let (_, encryption) = X25519HkdfSha256::derive_keypair(&[endpoint_seed; 32]);
        let fields = UnsignedAdvertisement {
            version: 1, network: NETWORK, product_id: PRODUCT.into(), account: account.public.to_bytes(),
            endpoint_id: [endpoint_seed; 32], encryption_key: encryption.to_bytes().into(),
            signing_key: signing.public.to_bytes(), issued_at: now, expires_at: now + 600,
        };
        let signature = account.sign_simple(ACCOUNT_SIGNING_CONTEXT, &fields.account_signing_input()).to_bytes();
        fields.authenticate(signature, now).unwrap()
    }

    fn failing(attempts: Arc<Mutex<Vec<Instant>>>) -> Connect {
        Arc::new(move |_: &MediaService| {
            attempts.lock().unwrap().push(Instant::now());
            async { Err(domain(v::HostMediaError::NotConnected)) }.boxed()
        })
    }

    /// Fails until `release`, then binds a transport-free endpoint for `local`.
    fn released(release: Arc<AtomicBool>, attempts: Arc<AtomicUsize>, local: VerifiedAdvertisement,
        bound: Arc<Mutex<Option<Arc<MediaSignaling>>>>) -> Connect {
        Arc::new(move |service: &MediaService| {
            attempts.fetch_add(1, Ordering::SeqCst);
            if !release.load(Ordering::SeqCst) { return async { Err(domain(v::HostMediaError::NotConnected)) }.boxed(); }
            let session = service.authority.current_session().expect("test session");
            let identity = MediaIdentity { network: NETWORK, product_id: PRODUCT.into(), account: local.advertisement().fields.account };
            let (signaling, events) = MediaSignaling::connected_for_test(service.services.clone(), service.authority.clone(),
                session.clone(), identity.clone(), local.clone());
            *bound.lock().unwrap() = Some(signaling.clone());
            async move { Ok(SignalingLink { session, identity, signaling, events }) }.boxed()
        })
    }

    #[test]
    fn backoff_doubles_from_min_and_stays_bounded() {
        let backoff = Backoff { min: Duration::from_millis(500), max: Duration::from_secs(10) };
        let mut delay = Duration::ZERO;
        let delays: Vec<_> = (0..8).map(|_| { delay = backoff.next(delay); delay.as_millis() }).collect();
        assert_eq!(delays, [500, 1000, 2000, 4000, 8000, 10_000, 10_000, 10_000]);
        assert_eq!(Backoff::DEFAULT.next(Duration::MAX), Backoff::DEFAULT.max);
    }

    #[test]
    fn snapshot_arrives_while_signaling_is_not_ready() {
        let connect: Connect = Arc::new(|_: &MediaService| futures::future::pending().boxed());
        let host = host(connect, Backoff::DEFAULT);
        let started = Instant::now();
        let mut subscription = subscribe(&host);
        assert!(matches!(event(next_within(&mut subscription, Duration::from_secs(1))),
            v::MediaEvent::Snapshot { sessions, incoming, .. } if sessions.is_empty() && incoming.is_empty()));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(host.media.get().unwrap().lock().signaling.is_none());
        // Pending signaling neither terminates nor fills the stream.
        assert!(next_within(&mut subscription, Duration::from_millis(300)).is_none());
        host.close_media();
        assert!(matches!(next_within(&mut subscription, Duration::from_secs(1)), Some(None)));
    }

    #[test]
    fn signaling_that_comes_up_later_delivers_an_incoming_offer() {
        let (release, attempts, bound) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(None)));
        let local = advertisement(1, 11);
        let host = host(released(release.clone(), attempts.clone(), local.clone(), bound.clone()),
            Backoff { min: Duration::from_millis(10), max: Duration::from_millis(40) });
        let mut subscription = subscribe(&host);
        assert!(matches!(event(next_within(&mut subscription, Duration::from_secs(1))), v::MediaEvent::Snapshot { .. }));
        wait_until(|| attempts.load(Ordering::SeqCst) >= 3, "signaling start is retried");
        assert!(host.media.get().unwrap().lock().signaling.is_none());
        release.store(true, Ordering::SeqCst);
        wait_until(|| host.media.get().unwrap().lock().signaling.is_some(), "signaling binds after recovery");

        let remote = advertisement(2, 21);
        let now = current_unix_secs();
        let mut caller = group::GroupEngine::new(remote.clone());
        caller.create_session([10; 32]).unwrap();
        let callee = v::MediaPeer { network: v::MediaNetwork { genesis_hash: NETWORK }, product_id: PRODUCT.into(),
            account: v::MediaAccount::Sr25519(local.advertisement().fields.account) };
        let invitation = caller.invite([10; 32], [11; 32], callee, vec![local], &state::off_tracks(), now).unwrap();
        let signaling = bound.lock().unwrap().clone().unwrap();
        for action in invitation {
            if let GroupAction::Send { payload, .. } = action {
                signaling.deliver_for_test(AuthenticatedMediaMessage { sender: remote.clone(), expires_at: now + 60, plaintext: payload }).unwrap();
            }
        }
        let offer = loop {
            match event(next_within(&mut subscription, Duration::from_secs(2))) {
                v::MediaEvent::IncomingOffered { offer, .. } => break offer,
                _ => continue,
            }
        };
        assert_eq!(offer.peer.account, v::MediaAccount::Sr25519(remote.advertisement().fields.account));
        host.close_media();
    }

    #[test]
    fn failed_starts_back_off_within_bounds_and_stop_on_runtime_close() {
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let backoff = Backoff { min: Duration::from_millis(20), max: Duration::from_millis(80) };
        let host = host(failing(attempts.clone()), backoff);
        let mut subscription = subscribe(&host);
        assert!(matches!(event(next_within(&mut subscription, Duration::from_secs(1))), v::MediaEvent::Snapshot { .. }));
        wait_until(|| attempts.lock().unwrap().len() >= 6, "failed signaling starts are retried");
        let gaps: Vec<_> = attempts.lock().unwrap().windows(2).map(|pair| pair[1] - pair[0]).take(5).collect();
        assert!(gaps[0] >= backoff.min, "first retry waits: {gaps:?}");
        assert!(gaps[2] >= Duration::from_millis(70), "retries back off: {gaps:?}");
        assert!(gaps.iter().all(|gap| *gap < backoff.max + Duration::from_millis(200)), "backoff is bounded: {gaps:?}");
        // The stream stays open while signaling is retried.
        assert!(next_within(&mut subscription, Duration::from_millis(50)).is_none());

        host.close_media();
        std::thread::sleep(backoff.max * 2);
        let after_close = attempts.lock().unwrap().len();
        std::thread::sleep(backoff.max * 3);
        assert_eq!(attempts.lock().unwrap().len(), after_close, "no signaling start after runtime close");
        assert!(matches!(next_within(&mut subscription, Duration::from_secs(1)), Some(None)));
    }

    #[test]
    fn an_operation_waits_for_signaling_instead_of_failing_at_once() {
        let (release, attempts, bound) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(None)));
        let host = Arc::new(host(released(release.clone(), attempts.clone(), advertisement(1, 11), bound),
            Backoff { min: Duration::from_millis(10), max: Duration::from_millis(40) }));
        let mut subscription = subscribe(&host);
        assert!(matches!(event(next_within(&mut subscription, Duration::from_secs(1))), v::MediaEvent::Snapshot { .. }));
        let creating = host.clone();
        let started = Instant::now();
        let create = std::thread::spawn(move || futures::executor::block_on(creating.create_session(&CallContext::default(),
            wire::HostMediaCreateSessionRequest::V1(v::HostMediaCreateSessionRequest { operation_id: [9; 32], tracks: state::off_tracks() }))));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!create.is_finished(), "creation waits for signaling");
        release.store(true, Ordering::SeqCst);
        let result = create.join().unwrap();
        assert!(started.elapsed() < OPERATION_TIMEOUT, "creation proceeds once signaling binds");
        assert!(matches!(result, Ok(wire::HostMediaCreateSessionResponse::V1(_))), "{result:?}");
        host.close_media();
    }
}
