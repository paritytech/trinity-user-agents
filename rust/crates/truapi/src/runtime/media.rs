//! Host-owned Media: one authority-fenced service per trusted product execution.

mod backend;
mod group;
mod operations;
mod state;

use std::{future::Future, sync::{Arc, Mutex, MutexGuard, Weak}, time::Duration};
use futures::{future::BoxFuture, pin_mut, select_biased, FutureExt, StreamExt};
use truapi::{api::Media, v01 as v, versioned::media as wire, CallContext, CallError, CancellationToken, Subscription};
use truapi::versioned::IntoLatest;
use crate::platform::{MediaBackendCommand, MediaBackendResponse, MediaPlatform, PermissionAuthorizationRequest, PermissionStatusHost, Platform, ProductContext, ProductExecutionKind};
use super::{authority::ProductAuthority, media_identity, media_signaling::{MediaSignaling, MediaSignalingError}, services::RuntimeServices, ProductRuntimeHost};
use crate::host_internal::permissions::PermissionsService;
use backend::{end_all_locked, revoke_permission_locked};
use group::GroupEngine;
use state::*;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

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
        let service = Arc::new_cyclic(|weak| Self {
            services: host.services.clone(), platform: host.platform.clone(), permission_status: host.permission_status.clone(),
            authority: host.authority.clone(), product: host.product.clone(), backend, runtime_id: host.core_instance,
            state: Mutex::new(State::new()), init: futures::lock::Mutex::new(()), closed: CancellationToken::default(), weak: weak.clone(),
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
    async fn ensure_ready(self: &Arc<Self>) -> Result<()> {
        self.capabilities().await?;
        {
            let state = self.lock();
            if self.current_authority(&state) && state.signaling.is_some() { return Ok(()); }
        }
        let cancel = CancellationToken::default();
        self.bounded(&cancel, OPERATION_TIMEOUT, false, async {
            let _guard = self.init.lock().await;
            let stale = {
                let state = self.lock();
                state.ensure_open()?;
                if self.current_authority(&state) && state.signaling.is_some() { return Ok(()); }
                state.binding.is_some()
            };
            if stale { self.lose_identity(v::MediaCallOutcome::IdentityLost); }
            let start_backend = {
                let mut state = self.lock();
                state.ensure_open()?;
                let start = !state.backend_started;
                state.backend_started = true;
                start
            };
            if start_backend { self.start_backend_observation(); }
            self.lock().ensure_open()?;
            let mut cx = CallContext::default();
            cx.set_timeout(OPERATION_TIMEOUT);
            let (session, identity) = media_identity::resolve_identity(self.services.as_ref(), self.authority.as_ref(), &self.product, &cx)
                .await.map_err(|_| domain(v::HostMediaError::NotConnected))?;
            let (signaling, mut events) = MediaSignaling::start(self.services.clone(), self.authority.clone(), session.clone(), identity.clone(), &cx)
                .await.map_err(signaling_error)?;
            let advertisement = signaling.advertisement().map_err(signaling_error)?;
            let epoch = {
                let mut state = self.lock();
                if state.ensure_open().is_err() || !self.authority.current_session().as_ref().is_some_and(|current| same_authority(current, &session)) {
                    signaling.close();
                    return Err(domain(v::HostMediaError::NotConnected));
                }
                state.binding = Some(Binding { session, identity });
                state.group = Some(GroupEngine::new(advertisement));
                state.signaling = Some(signaling.clone());
                state.epoch
            };
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
                    if service.lock().epoch != epoch { break; }
                    match event {
                        Ok(event) => service.signaling_event(epoch, event),
                        Err(_) => {
                            service.lose_identity(v::MediaCallOutcome::ConnectivityLost);
                            break;
                        }
                    }
                }
                if let Some(service) = weak.upgrade()
                    && service.lock().epoch == epoch { service.lose_identity(v::MediaCallOutcome::ConnectivityLost); }
                guard.complete();
            }.boxed());
            Ok(())
        }).await
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
        if let Err(error) = service.ensure_ready().await { return Subscription::interrupted(map_error(error, convert)); }
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
