//! One product connection's `JamPeerTransport` over JAMNP-S QUIC.
//!
//! `dial` requires `RemotePermission::JamPeers { genesis }`: the caller hands
//! in the permission check (stored decision, else prompt, then persist), and
//! the session runs it once per genesis for this connection and keeps the
//! answer, so concurrent dials wait for one prompt and a refusal stays
//! `NotGranted` without asking again. The check runs on the runtime spawner,
//! so its answer is kept even when every dial waiting on it has given up.
//! Pending dials reserve from the eight-connection budget before permission
//! is awaited. At most eight distinct genesis decisions are kept, including
//! pending and refused checks; a ninth is `Limit`, with no eviction or prompt.
//!
//! A dial answers within [`DIAL_DEADLINE`], prompt included: one still waiting
//! then answers `Unreachable`, a cancelled one `Cancelled`, and whatever it
//! would have opened is dropped without holding a connection slot. The QUIC
//! endpoint is created by the first granted dial, so a connection that never
//! dials, or is refused, binds no socket. After [`JamPeerSession::revoke`]
//! every call is `Denied`.

use core::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures::{FutureExt, pin_mut};
use parking_lot::Mutex;
use tokio::sync::watch;
use truapi::versioned::jam_peer_transport as wire;
use truapi::{CallContext, CallError, latest};

use super::quic::{self, Dial, Transport};
use crate::subscription::Spawner;

/// Bound on one `dial`, from its arrival to its reply, the permission prompt
/// included. A guest that waits at least this long for a dial reply never
/// misses one, and anything a dial would open after it is closed instead.
const DIAL_DEADLINE: Duration = Duration::from_secs(10);

/// Largest reply frame a native PolkaVM runtime accepts
/// (`polkavm_host_runtime::MAX_HOST_FRAME_BYTES`); a `recv` reply carries its
/// message inside one, next to the request id and the SCALE envelope.
const MAX_REPLY_FRAME_BYTES: usize = 1024 * 1024;
/// Room left in a `recv` reply frame for everything but the message.
const REPLY_ENVELOPE_BYTES: usize = 128;

/// The answer to `RemotePermission::JamPeers` for one genesis.
type Decision = Result<(), CallError<wire::HostJamPeerTransportDialError>>;

/// One product connection's peer transport.
pub(crate) struct JamPeerSession {
    /// Created by the first granted dial; `None` again after [`Self::revoke`].
    transport: Mutex<Option<Arc<Transport>>>,
    /// Permission answers for this connection, by genesis.
    decisions: Mutex<HashMap<[u8; 32], watch::Receiver<Option<Decision>>>>,
    /// Dials still awaiting permission or a handshake.
    pending_dials: AtomicUsize,
    dial_deadline: Duration,
    revoked: AtomicBool,
    revoked_signal: truapi::CancellationToken,
}

/// A pending dial reserves capacity before authorization and releases it on
/// every completion path. Successful connections are then counted by QUIC.
struct DialAdmission<'a> {
    slots: &'a AtomicUsize,
}

impl<'a> DialAdmission<'a> {
    fn reserve(
        slots: &'a AtomicUsize,
        transport: Option<&Transport>,
    ) -> Result<Self, CallError<wire::HostJamPeerTransportDialError>> {
        let admitted = match transport {
            Some(transport) => transport.reserve_pending_dial(slots),
            None => slots
                .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    (used < quic::MAX_CONNECTIONS).then_some(used + 1)
                })
                .is_ok(),
        };
        if !admitted {
            return Err(dial_error(latest::HostJamPeerTransportDialError::Limit));
        }
        Ok(Self { slots })
    }
}

impl Drop for DialAdmission<'_> {
    fn drop(&mut self) {
        self.slots.fetch_sub(1, Ordering::AcqRel);
    }
}

fn dial_error(
    error: latest::HostJamPeerTransportDialError,
) -> CallError<wire::HostJamPeerTransportDialError> {
    CallError::Domain(wire::HostJamPeerTransportDialError::V1(error))
}

impl JamPeerSession {
    /// A session with no endpoint and no permission answers yet.
    pub(crate) fn new() -> Self {
        Self {
            transport: Mutex::new(None),
            decisions: Mutex::new(HashMap::new()),
            pending_dials: AtomicUsize::new(0),
            dial_deadline: DIAL_DEADLINE,
            revoked: AtomicBool::new(false),
            revoked_signal: truapi::CancellationToken::default(),
        }
    }

    /// Close every connection; every later call, including one still waiting
    /// for a handshake or a permission prompt, is `Denied`.
    pub(crate) fn revoke(&self) {
        let transport = {
            let mut transport = self.transport.lock();
            self.revoked.store(true, Ordering::Release);
            transport.take()
        };
        self.revoked_signal.cancel();
        if let Some(transport) = transport {
            transport.shutdown();
        }
    }

    fn live<E>(&self) -> Result<(), CallError<E>> {
        if self.revoked.load(Ordering::Acquire) {
            Err(CallError::Denied)
        } else {
            Ok(())
        }
    }

    /// The endpoint, if a granted dial has created it.
    fn existing(&self) -> Option<Arc<Transport>> {
        self.transport.lock().clone()
    }

    /// The endpoint, created on first use. Never created after a revoke.
    fn endpoint(&self) -> Result<Arc<Transport>, CallError<wire::HostJamPeerTransportDialError>> {
        let mut transport = self.transport.lock();
        self.live()?;
        if let Some(transport) = &*transport {
            return Ok(transport.clone());
        }
        let created = Arc::new(Transport::new().map_err(|error| CallError::HostFailure {
            reason: format!("JAM peer transport unavailable: {error}"),
        })?);
        *transport = Some(created.clone());
        Ok(created)
    }

    /// `JamPeers { genesis }` for this connection, checked once.
    async fn permitted<F>(
        &self,
        genesis: [u8; 32],
        authorize: impl FnOnce() -> F,
        spawner: &Spawner,
    ) -> Decision
    where
        F: Future<Output = Decision> + Send + 'static,
    {
        let (mut decision, first) = {
            let mut decisions = self.decisions.lock();
            match decisions.get(&genesis) {
                Some(decision) => (decision.clone(), None),
                None => {
                    if decisions.len() >= quic::MAX_CONNECTIONS {
                        return Err(dial_error(latest::HostJamPeerTransportDialError::Limit));
                    }
                    let (answer, decision) = watch::channel(None);
                    decisions.insert(genesis, decision.clone());
                    (decision, Some(answer))
                }
            }
        };
        if let Some(answer) = first {
            let check = authorize().fuse();
            let revoked = self.revoked_signal.cancelled().fuse();
            spawner(Box::pin(async move {
                pin_mut!(check, revoked);
                let result = futures::select_biased! {
                    _ = revoked => Err(CallError::Denied),
                    result = check => result,
                };
                let _ = answer.send(Some(result));
            }));
        }
        match decision.wait_for(Option::is_some).await {
            Ok(answer) => answer.clone().expect("waited for an answer"),
            // A check that ended without answering refuses.
            Err(_) => Err(dial_error(
                latest::HostJamPeerTransportDialError::NotGranted,
            )),
        }
    }

    async fn dial_granted<F>(
        &self,
        request: latest::HostJamPeerTransportDialRequest,
        authorize: impl FnOnce() -> F,
        spawner: &Spawner,
    ) -> Result<
        wire::HostJamPeerTransportDialResponse,
        CallError<wire::HostJamPeerTransportDialError>,
    >
    where
        F: Future<Output = Decision> + Send + 'static,
    {
        let _admission = {
            // Serialize admission with endpoint creation and revocation.
            let transport = self.transport.lock();
            self.live()?;
            DialAdmission::reserve(&self.pending_dials, transport.as_deref())?
        };
        self.permitted(request.genesis, authorize, spawner).await?;
        let transport = self.endpoint()?;
        // Native hosts speak JAMNP-S QUIC; the P-256 id is for WebTransport hosts.
        let conn = transport
            .dial(&Dial {
                genesis: request.genesis,
                ip: request.ip,
                port: request.port,
                ed25519: request.ed25519,
            })
            .await
            .map_err(|error| {
                dial_error(match error {
                    quic::DialError::Refused => latest::HostJamPeerTransportDialError::Refused,
                    quic::DialError::Limit => latest::HostJamPeerTransportDialError::Limit,
                    quic::DialError::Unreachable => {
                        latest::HostJamPeerTransportDialError::Unreachable
                    }
                })
            })?;
        if self.revoked.load(Ordering::Acquire) {
            // Revoked while the handshake ran: the connection must not outlive it.
            let _ = transport.close(conn);
            return Err(CallError::Denied);
        }
        Ok(wire::HostJamPeerTransportDialResponse::V1(
            latest::HostJamPeerTransportDialResponse { conn },
        ))
    }

    /// Dial one peer once `authorize` grants its genesis. `authorize` runs at
    /// most once per genesis for this connection.
    pub(crate) async fn dial<F>(
        &self,
        cx: &CallContext,
        request: wire::HostJamPeerTransportDialRequest,
        authorize: impl FnOnce() -> F,
        spawner: &Spawner,
    ) -> Result<
        wire::HostJamPeerTransportDialResponse,
        CallError<wire::HostJamPeerTransportDialError>,
    >
    where
        F: Future<Output = Decision> + Send + 'static,
    {
        self.live()?;
        let wire::HostJamPeerTransportDialRequest::V1(request) = request;
        // Dropping `granted` early drops its pending handshake, whose
        // connection slot is released with it.
        let granted = self.dial_granted(request, authorize, spawner).fuse();
        let cancelled = cx.cancel().cancelled().fuse();
        let deadline = futures_timer::Delay::new(self.dial_deadline).fuse();
        let revoked = self.revoked_signal.cancelled().fuse();
        pin_mut!(granted, cancelled, deadline, revoked);
        futures::select_biased! {
            _ = revoked => Err(CallError::Denied),
            _ = cancelled => Err(CallError::Cancelled),
            reply = granted => reply,
            () = deadline => Err(dial_error(latest::HostJamPeerTransportDialError::Unreachable)),
        }
    }

    /// Open a stream on a connection a granted dial opened.
    pub(crate) async fn open(
        &self,
        cx: &CallContext,
        request: wire::HostJamPeerTransportOpenRequest,
    ) -> Result<
        wire::HostJamPeerTransportOpenResponse,
        CallError<wire::HostJamPeerTransportOpenError>,
    > {
        self.live()?;
        let wire::HostJamPeerTransportOpenRequest::V1(request) = request;
        let closed = || {
            CallError::Domain(wire::HostJamPeerTransportOpenError::V1(
                latest::HostJamPeerTransportOpenError::Closed,
            ))
        };
        let transport = self.existing().ok_or_else(closed)?;
        let opened = transport.open(request.conn, request.kind).fuse();
        let cancelled = cx.cancel().cancelled().fuse();
        let revoked = self.revoked_signal.cancelled().fuse();
        pin_mut!(opened, cancelled, revoked);
        let result = futures::select_biased! {
            _ = revoked => return Err(CallError::Denied),
            _ = cancelled => return Err(CallError::Cancelled),
            result = opened => result,
        };
        let stream = result.map_err(|error| match error {
            quic::OpenError::Closed => closed(),
            quic::OpenError::Limit => {
                CallError::Domain(wire::HostJamPeerTransportOpenError::V1(
                    latest::HostJamPeerTransportOpenError::Limit,
                ))
            }
        })?;
        if self.revoked.load(Ordering::Acquire) {
            let _ = transport.reset(stream);
            return Err(CallError::Denied);
        }
        Ok(wire::HostJamPeerTransportOpenResponse::V1(
            latest::HostJamPeerTransportOpenResponse { stream },
        ))
    }

    /// Queue one message on a stream.
    pub(crate) fn send(
        &self,
        request: wire::HostJamPeerTransportSendRequest,
    ) -> Result<
        wire::HostJamPeerTransportSendResponse,
        CallError<wire::HostJamPeerTransportSendError>,
    > {
        self.live()?;
        let wire::HostJamPeerTransportSendRequest::V1(request) = request;
        let domain = |error| CallError::Domain(wire::HostJamPeerTransportSendError::V1(error));
        self.existing()
            .ok_or(quic::SendError::Closed)
            .and_then(|transport| transport.send(request.stream, &request.message, request.fin))
            .map_err(|error| {
                domain(match error {
                    quic::SendError::Closed => latest::HostJamPeerTransportSendError::Closed,
                    quic::SendError::TooLarge => latest::HostJamPeerTransportSendError::TooLarge,
                    quic::SendError::Limit => latest::HostJamPeerTransportSendError::Limit,
                })
            })?;
        Ok(wire::HostJamPeerTransportSendResponse::V1)
    }

    /// Poll one message from a stream.
    pub(crate) fn recv(
        &self,
        request: wire::HostJamPeerTransportRecvRequest,
    ) -> Result<
        wire::HostJamPeerTransportRecvResponse,
        CallError<wire::HostJamPeerTransportRecvError>,
    > {
        self.live()?;
        let wire::HostJamPeerTransportRecvRequest::V1(request) = request;
        let max = (request.max as usize).min(MAX_REPLY_FRAME_BYTES - REPLY_ENVELOPE_BYTES);
        let received = self
            .existing()
            .ok_or(quic::Closed)
            .and_then(|transport| transport.recv(request.stream, max))
            .map_err(|quic::Closed| {
                CallError::Domain(wire::HostJamPeerTransportRecvError::V1(
                    latest::HostJamPeerTransportRecvError::Closed,
                ))
            })?;
        Ok(wire::HostJamPeerTransportRecvResponse::V1(
            latest::HostJamPeerTransportRecvResponse {
                message: received.message,
                fin: received.fin,
                reset: received.reset,
            },
        ))
    }

    /// Abort a stream in both directions.
    pub(crate) fn reset(
        &self,
        request: wire::HostJamPeerTransportResetRequest,
    ) -> Result<
        wire::HostJamPeerTransportResetResponse,
        CallError<wire::HostJamPeerTransportResetError>,
    > {
        self.live()?;
        let wire::HostJamPeerTransportResetRequest::V1(request) = request;
        self.existing()
            .ok_or(quic::Closed)
            .and_then(|transport| transport.reset(request.stream))
            .map_err(|quic::Closed| {
                CallError::Domain(wire::HostJamPeerTransportResetError::V1(
                    latest::HostJamPeerTransportResetError::Closed,
                ))
            })?;
        Ok(wire::HostJamPeerTransportResetResponse::V1)
    }

    /// Close a connection and every stream on it.
    pub(crate) fn close(
        &self,
        request: wire::HostJamPeerTransportCloseRequest,
    ) -> Result<
        wire::HostJamPeerTransportCloseResponse,
        CallError<wire::HostJamPeerTransportCloseError>,
    > {
        self.live()?;
        let wire::HostJamPeerTransportCloseRequest::V1(request) = request;
        self.existing()
            .ok_or(quic::Closed)
            .and_then(|transport| transport.close(request.conn))
            .map_err(|quic::Closed| {
                CallError::Domain(wire::HostJamPeerTransportCloseError::V1(
                    latest::HostJamPeerTransportCloseError::Closed,
                ))
            })?;
        Ok(wire::HostJamPeerTransportCloseResponse::V1)
    }

    /// Drain pending events.
    pub(crate) fn events(
        &self,
    ) -> Result<
        wire::HostJamPeerTransportEventsResponse,
        CallError<wire::HostJamPeerTransportEventsError>,
    > {
        self.live()?;
        let events = self
            .existing()
            .map(|transport| transport.events())
            .unwrap_or_default();
        Ok(wire::HostJamPeerTransportEventsResponse::V1(
            latest::HostJamPeerTransportEventsResponse { events },
        ))
    }
}

#[cfg(test)]
mod tests;

impl Drop for JamPeerSession {
    fn drop(&mut self) {
        self.revoke();
    }
}
