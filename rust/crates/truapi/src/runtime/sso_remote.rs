//! SSO remote messaging over the people-chain statement store: submits an
//! encrypted request statement to the paired signing host and waits for the
//! matching response, honoring timeouts and local/peer disconnect signals.

use core::mem;
use std::fmt::{self, Display};
use std::sync::Mutex;

use super::statement_store_rpc;
use crate::host_internal::sso_messages::{
    Response, SsoSessionStatement, decode_sso_session_statement, v1,
};
use crate::host_internal::sso_wire::SsoRequest;
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::statement_store::parse_new_statements_result;
use crate::unix_time::current_unix_secs;

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use serde_json::Value;
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::RpcSubscription;
use tracing::{debug, instrument};
use truapi::{CancellationReason, CancellationToken};

/// Host-spec B.3.3 recommends seven-day statement expiry for session traffic:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L143-L145>
pub const DEFAULT_SSO_STATEMENT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
/// The statement store keeps only the highest-priority statement on a channel.
/// Expiry's lower 32 bits are the tie-breaker for statements expiring in the
/// same second, so every submission from this process must advance them.
static LAST_SSO_STATEMENT_EXPIRY: Mutex<u64> = Mutex::new(0);
/// Disconnect reason reported when the local session logs out mid-request.
pub const SSO_LOCAL_DISCONNECT_REASON: &str = "SSO session disconnected";
/// Disconnect reason reported when the paired signing host announces a disconnect.
pub const SSO_PEER_DISCONNECT_REASON: &str = "SSO peer disconnected";
/// Reason reported when the product caller cancels a pending SSO request.
const SSO_CALL_CANCELLED_REASON: &str = "SSO response wait cancelled by caller";

/// Registry of oneshot waiters resolved when the SSO session disconnects.
#[derive(Default)]
pub struct SessionDisconnects {
    inner: Mutex<SessionDisconnectsInner>,
}

#[derive(Default)]
struct SessionDisconnectsInner {
    next_id: u64,
    waiters: Vec<(u64, SsoSessionKey, oneshot::Sender<String>)>,
}

/// Identifies one SSO session by its own and peer session ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SsoSessionKey {
    own: [u8; 32],
    peer: [u8; 32],
}

impl SsoSessionKey {
    /// Key for the session's own/peer id pair.
    pub fn from_session(session: &SsoSessionInfo) -> Self {
        Self {
            own: session.session_id_own,
            peer: session.session_id_peer,
        }
    }
}

/// Unregisters a disconnect waiter when the waiting call finishes.
pub struct SessionDisconnectGuard {
    disconnects: std::sync::Arc<SessionDisconnects>,
    id: u64,
}

impl Drop for SessionDisconnectGuard {
    fn drop(&mut self) {
        self.disconnects.unsubscribe(self.id);
    }
}

impl SessionDisconnects {
    /// Register a waiter; returns its id and the disconnect-reason receiver.
    pub fn subscribe(
        self: &std::sync::Arc<Self>,
        session: &SsoSessionInfo,
    ) -> (SessionDisconnectGuard, oneshot::Receiver<String>) {
        let (tx, rx) = oneshot::channel();
        let mut inner = self
            .inner
            .lock()
            .expect("session disconnect mutex poisoned");
        inner.next_id = inner.next_id.wrapping_add(1);
        let id = inner.next_id;
        inner
            .waiters
            .push((id, SsoSessionKey::from_session(session), tx));
        (
            SessionDisconnectGuard {
                disconnects: self.clone(),
                id,
            },
            rx,
        )
    }

    fn unsubscribe(&self, id: u64) {
        self.inner
            .lock()
            .expect("session disconnect mutex poisoned")
            .waiters
            .retain(|(waiter_id, _, _)| *waiter_id != id);
    }

    /// Resolve pending waiters for one SSO session with `reason`.
    pub fn notify(&self, session: &SsoSessionInfo, reason: &'static str) {
        self.notify_key(SsoSessionKey::from_session(session), reason);
    }

    /// Resolve pending waiters for the session identified by `key`.
    pub fn notify_key(&self, key: SsoSessionKey, reason: &'static str) {
        let waiters = {
            let mut inner = self
                .inner
                .lock()
                .expect("session disconnect mutex poisoned");
            let mut matching = Vec::new();
            let mut pending = Vec::with_capacity(inner.waiters.len());
            for waiter in mem::take(&mut inner.waiters) {
                if waiter.1 == key {
                    matching.push(waiter);
                } else {
                    pending.push(waiter);
                }
            }
            inner.waiters = pending;
            matching
        };
        for (_, _, waiter) in waiters {
            let _ = waiter.send(reason.to_string());
        }
    }
}

/// Stream of raw statement-store notification pages.
pub type StatementPageStream = BoxStream<'static, Result<Value, String>>;
/// Future resolving when the request statement submit completes.
pub type StatementSubmitFuture = BoxFuture<'static, Result<(), SsoRemoteResponseError>>;

/// Inputs for one remote-response wait.
pub struct RemoteResponseWait<'a> {
    /// Statement pages on the session's own topic.
    pub own_statements: StatementPageStream,
    /// Statement pages on the peer's topic.
    pub peer_statements: StatementPageStream,
    /// Submit of the request statement, raced alongside the response wait.
    pub submit: StatementSubmitFuture,
    /// Session the response must decrypt against.
    pub session: &'a SsoSessionInfo,
    /// Request id embedded in the outgoing statement.
    pub statement_request_id: &'a str,
    /// Message id the matching response must carry.
    pub remote_message_id: &'a str,
    /// Caller-driven cancellation (timeout or explicit cancel).
    pub cancel: &'a CancellationToken,
    /// Resolves with a reason when the session disconnects, if registered.
    pub disconnect: Option<oneshot::Receiver<String>>,
}

/// Cancellation of a pending SSO request, tagged with the message id it
/// interrupted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelError {
    reason: CancellationReason,
    remote_message_id: String,
}

impl CancelError {
    fn new(reason: CancellationReason, remote_message_id: &str) -> Self {
        Self {
            reason,
            remote_message_id: remote_message_id.to_string(),
        }
    }

    /// Why the request was cancelled.
    pub fn reason(&self) -> CancellationReason {
        self.reason.clone()
    }

    /// Message id of the interrupted request.
    pub fn remote_message_id(&self) -> &str {
        &self.remote_message_id
    }

    /// Same cancellation reattributed to another message id.
    pub fn with_remote_message_id(self, remote_message_id: &str) -> Self {
        Self {
            reason: self.reason,
            remote_message_id: remote_message_id.to_string(),
        }
    }
}

impl Display for CancelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reason {
            CancellationReason::Cancelled => {
                write!(
                    f,
                    "{SSO_CALL_CANCELLED_REASON} for {}",
                    self.remote_message_id
                )
            }
            reason @ CancellationReason::TimedOut { .. } => {
                write!(f, "SSO response {reason} for {}", self.remote_message_id)
            }
        }
    }
}

/// Why a remote-response wait ended without a response.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::From)]
pub enum SsoRemoteResponseError {
    /// Caller cancelled or the wait timed out.
    #[display("{_0}")]
    Cancelled(CancelError),
    /// The local session logged out mid-request.
    #[display("{}", SSO_LOCAL_DISCONNECT_REASON)]
    LocalDisconnected,
    /// The paired signing host announced a disconnect.
    #[display("{}", SSO_PEER_DISCONNECT_REASON)]
    PeerDisconnected,
    /// Submit, subscription, or decode failure.
    #[display("{_0}")]
    #[from]
    Failure(String),
}

fn disconnect_error(reason: String) -> SsoRemoteResponseError {
    match reason.as_str() {
        SSO_LOCAL_DISCONNECT_REASON => SsoRemoteResponseError::LocalDisconnected,
        SSO_PEER_DISCONNECT_REASON => SsoRemoteResponseError::PeerDisconnected,
        _ => SsoRemoteResponseError::Failure(reason),
    }
}

/// Matcher for [`wait_for_sso_remote_response`]: the response to the request
/// sent as `message_id`. A response addressed to it but of another kind is an
/// error, so a confused peer fails the call instead of stalling it.
pub fn reply_matcher<R: SsoRequest>(
    message_id: &str,
) -> impl Fn(v1::RemoteMessage) -> Option<Result<Response<R::Response>, String>> + '_ {
    move |message| {
        if message.responding_to() != Some(message_id) {
            return None;
        }
        let kind = message.name();
        Some(
            R::response_from_message(message)
                .ok_or_else(|| format!("Unexpected SSO response for {}: {kind}", R::NAME)),
        )
    }
}

/// Wait for the reply `matches` accepts, racing the statement streams against
/// submit failure, cancellation, and disconnect signals.
///
/// `matches` sees every peer message in wire order: `None` skips a message
/// meant for someone else, `Some(Err(reason))` fails the wait for a message
/// addressed to this request but of the wrong kind.
#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait"))]
pub async fn wait_for_sso_remote_response<T>(
    wait: RemoteResponseWait<'_>,
    matches: impl Fn(v1::RemoteMessage) -> Option<Result<T, String>>,
) -> Result<T, SsoRemoteResponseError> {
    let RemoteResponseWait {
        own_statements,
        peer_statements,
        submit,
        session,
        statement_request_id,
        remote_message_id,
        cancel,
        disconnect,
    } = wait;
    let response = wait_for_sso_remote_response_inner(
        own_statements,
        peer_statements,
        submit,
        session,
        statement_request_id,
        remote_message_id,
        &matches,
    )
    .fuse();
    let disconnect = async move {
        match disconnect {
            Some(rx) => {
                match rx.await {
                    Ok(reason) => disconnect_error(reason),
                    Err(_) => SsoRemoteResponseError::LocalDisconnected,
                }
            }
            None => futures::future::pending::<SsoRemoteResponseError>().await,
        }
    }
    .fuse();
    let cancel_message_id = remote_message_id.to_string();
    let cancelled = async move {
        let reason = cancel.cancelled().await;
        SsoRemoteResponseError::Cancelled(CancelError::new(reason, &cancel_message_id))
    }
    .fuse();
    pin_mut!(response, disconnect, cancelled);
    // Cancellation first: a call withdrawn while it was still setting up must
    // not submit its request on the way out.
    futures::select_biased! {
        reason = cancelled => Err(reason),
        result = response => result,
        reason = disconnect => Err(reason),
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait_inner"))]
async fn wait_for_sso_remote_response_inner<T>(
    own_statements: StatementPageStream,
    peer_statements: StatementPageStream,
    submit: StatementSubmitFuture,
    session: &SsoSessionInfo,
    statement_request_id: &str,
    remote_message_id: &str,
    matches: &impl Fn(v1::RemoteMessage) -> Option<Result<T, String>>,
) -> Result<T, SsoRemoteResponseError> {
    let mut own_statements = own_statements.fuse();
    let mut peer_statements = peer_statements.fuse();
    let mut submit = submit.fuse();
    let mut own_done = false;
    let mut peer_done = false;
    let mut request_accepted = false;
    let mut pending_remote_response = None;

    loop {
        if own_done && peer_done {
            return Err(SsoRemoteResponseError::Failure(format!(
                "SSO response stream ended before response for {}",
                remote_message_id
            )));
        }
        futures::select! {
            item = own_statements.next() => {
                match item {
                    Some(Ok(value)) => {
                        if let Some(response) = handle_sso_remote_statement_page(
                            session,
                            &value,
                            statement_request_id,
                            matches,
                            &mut request_accepted,
                            &mut pending_remote_response,
                        )? {
                            return Ok(response);
                        }
                    }
                    Some(Err(reason)) => return Err(SsoRemoteResponseError::Failure(reason)),
                    None => own_done = true,
                }
            }
            item = peer_statements.next() => {
                match item {
                    Some(Ok(value)) => {
                        if let Some(response) = handle_sso_remote_statement_page(
                            session,
                            &value,
                            statement_request_id,
                            matches,
                            &mut request_accepted,
                            &mut pending_remote_response,
                        )? {
                            return Ok(response);
                        }
                    }
                    Some(Err(reason)) => return Err(SsoRemoteResponseError::Failure(reason)),
                    None => peer_done = true,
                }
            }
            submit_result = submit => {
                submit_result?;
            }
        }
    }
}

fn handle_sso_remote_statement_page<T>(
    session: &SsoSessionInfo,
    value: &Value,
    statement_request_id: &str,
    matches: &impl Fn(v1::RemoteMessage) -> Option<Result<T, String>>,
    request_accepted: &mut bool,
    pending_remote_response: &mut Option<T>,
) -> Result<Option<T>, SsoRemoteResponseError> {
    let page = parse_new_statements_result("sso-remote".to_string(), value)
        .map_err(|err| SsoRemoteResponseError::Failure(err.to_string()))?;
    for statement in page.statements {
        match decode_sso_session_statement(session, &statement, statement_request_id)
            .map_err(SsoRemoteResponseError::Failure)?
        {
            Some(SsoSessionStatement::RequestAccepted) => {
                *request_accepted = true;
                if let Some(response) = pending_remote_response.take() {
                    return Ok(Some(response));
                }
            }
            Some(SsoSessionStatement::RemoteMessages(messages)) => {
                for message in messages {
                    let message = match message {
                        Ok(message) => message,
                        Err(error) => {
                            debug!(%error, "skipping undecodable SSO remote message");
                            continue;
                        }
                    };
                    if message == v1::RemoteMessage::Disconnected {
                        return Err(SsoRemoteResponseError::PeerDisconnected);
                    }
                    let Some(matched) = matches(message) else {
                        continue;
                    };
                    let response = matched.map_err(SsoRemoteResponseError::Failure)?;
                    if *request_accepted {
                        return Ok(Some(response));
                    }
                    *pending_remote_response = Some(response);
                    break;
                }
            }
            None => {}
        }
    }
    Ok(None)
}

/// Live statement-store subscription for a single topic.
pub async fn subscribe_statement_topic(
    rpc_client: &RpcClient,
    topic: [u8; 32],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    statement_store_rpc::subscribe_match_all(rpc_client, &[topic]).await
}

/// Adapt a subscription into a page stream, labelling errors with `label`.
pub fn statement_subscription_stream(
    subscription: RpcSubscription<Value>,
    label: &'static str,
) -> StatementPageStream {
    subscription
        .map(move |item| item.map_err(|err| format!("SSO {label} subscription failed: {err}")))
        .boxed()
}

/// Fresh opaque message id for one SSO request.
pub fn sso_message_id() -> String {
    nanoid::nanoid!(8)
}

/// Statement expiry field for a new SSO statement: unix expiry seconds in the
/// high 32 bits, seven days from now.
pub fn fresh_statement_expiry() -> u64 {
    let timestamp = current_unix_secs().saturating_add(DEFAULT_SSO_STATEMENT_EXPIRY_SECS);
    let expiry_floor = timestamp << 32;
    let mut last = LAST_SSO_STATEMENT_EXPIRY
        .lock()
        .expect("SSO statement expiry mutex poisoned");
    let expiry = next_statement_expiry(*last, expiry_floor);
    *last = expiry;
    expiry
}

fn next_statement_expiry(last: u64, expiry_floor: u64) -> u64 {
    expiry_floor.max(last.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_internal::sso_messages::{
        ProductSubtreeRequest, ProductSubtreeResponse, RemoteMessage, RemoteMessageData,
        build_outgoing_request_statement, build_signed_session_response_statement,
    };
    use crate::host_logic::sso::pairing::{SsoStatementData, encrypt_session_statement_data};
    use crate::host_logic::statement_store::build_signed_session_request_statement;
    use crate::test_support::{
        new_statements_frame, sso_host_and_responder_sessions, sso_session_info,
    };
    use futures::stream;
    use parity_scale_codec::Encode;
    use std::time::Duration;

    #[test]
    fn sso_message_id_uses_short_opaque_nanoids() {
        let first = sso_message_id();
        let second = sso_message_id();

        assert_eq!(first.len(), 8);
        assert_eq!(second.len(), 8);
        assert_ne!(first, "p:1");
        assert_ne!(second, "p:1");
        assert_ne!(first, second);
        assert!(first.bytes().all(is_nanoid_safe_byte));
        assert!(second.bytes().all(is_nanoid_safe_byte));
    }

    #[test]
    fn fresh_statement_expiry_is_strictly_monotonic() {
        let first = fresh_statement_expiry();
        let second = fresh_statement_expiry();

        assert!(second > first);
        let floor = 42_u64 << 32;
        assert_eq!(next_statement_expiry(floor, floor), floor + 1);
        assert_eq!(next_statement_expiry(floor + 7, floor), floor + 8);
    }

    fn is_nanoid_safe_byte(value: u8) -> bool {
        value.is_ascii_alphanumeric() || value == b'_' || value == b'-'
    }

    fn pending_wait<'a>(
        session: &'a SsoSessionInfo,
        cancel: &'a CancellationToken,
    ) -> RemoteResponseWait<'a> {
        RemoteResponseWait {
            own_statements: stream::pending().boxed(),
            peer_statements: stream::pending().boxed(),
            submit: futures::future::pending().boxed(),
            session,
            statement_request_id: "request-1",
            remote_message_id: "request-1",
            cancel,
            disconnect: None,
        }
    }

    #[test]
    fn sso_remote_response_waiter_reports_timeout_cancellation() {
        let session = sso_session_info();
        let cancel = CancellationToken::default();
        cancel.cancel_with_reason(CancellationReason::TimedOut {
            timeout: Duration::from_millis(1),
        });
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            pending_wait(session.sso.as_ref().unwrap(), &cancel),
            |_| None::<Result<(), String>>,
        ))
        .unwrap_err();

        let SsoRemoteResponseError::Cancelled(err) = err else {
            panic!("expected cancellation error");
        };
        assert_eq!(
            err.to_string(),
            "SSO response timed out after 1ms for request-1"
        );
    }

    #[test]
    fn sso_remote_response_waiter_reports_submit_rejections() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            RemoteResponseWait {
                submit: futures::future::ready(Err(SsoRemoteResponseError::Failure(
                    "SSO statement submit failed: no allowance".to_string(),
                )))
                .boxed(),
                ..pending_wait(session.sso.as_ref().unwrap(), &CancellationToken::default())
            },
            |_| None::<Result<(), String>>,
        ))
        .unwrap_err();

        assert_eq!(
            err,
            SsoRemoteResponseError::Failure(
                "SSO statement submit failed: no allowance".to_string()
            )
        );
    }

    #[test]
    fn sso_remote_response_waiter_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            RemoteResponseWait {
                disconnect: Some(rx),
                ..pending_wait(session.sso.as_ref().unwrap(), &CancellationToken::default())
            },
            |_| None::<Result<(), String>>,
        ))
        .unwrap_err();

        assert_eq!(err, SsoRemoteResponseError::LocalDisconnected);
    }

    #[test]
    fn sso_remote_response_waiter_stops_on_call_cancellation() {
        let session = sso_session_info();
        let cancel = CancellationToken::default();
        let wait = wait_for_sso_remote_response(
            pending_wait(session.sso.as_ref().unwrap(), &cancel),
            |_| None::<Result<(), String>>,
        );

        cancel.cancel();
        let err = futures::executor::block_on(wait).unwrap_err();

        let SsoRemoteResponseError::Cancelled(err) = err else {
            panic!("expected cancellation error");
        };
        assert_eq!(
            err.to_string(),
            "SSO response wait cancelled by caller for request-1"
        );
    }

    fn peer_page(statement: Vec<u8>) -> Value {
        let frame: Value = serde_json::from_str(&new_statements_frame("sub", vec![statement]))
            .expect("frame is json");
        frame["params"]["result"].clone()
    }

    fn subtree_response(responding_to: &str) -> RemoteMessage {
        RemoteMessage {
            message_id: "resp-1".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(Response {
                responding_to: responding_to.to_string(),
                payload: Ok([7; 32]),
            })),
        }
    }

    fn wait_for_subtree(
        host: &SsoSessionInfo,
        pages: Vec<Value>,
    ) -> Result<Response<ProductSubtreeResponse>, SsoRemoteResponseError> {
        let cancel = CancellationToken::default();
        futures::executor::block_on(wait_for_sso_remote_response(
            RemoteResponseWait {
                own_statements: stream::empty().boxed(),
                peer_statements: stream::iter(pages.into_iter().map(Ok)).boxed(),
                submit: futures::future::ready(Ok(())).boxed(),
                ..pending_wait(host, &cancel)
            },
            reply_matcher::<ProductSubtreeRequest>("request-1"),
        ))
    }

    /// A peer that answers with another response kind must fail the call at
    /// once; skipping it would leave the product waiting for the timeout.
    #[test]
    fn a_reply_of_the_wrong_kind_fails_instead_of_waiting() {
        let (host, responder) = sso_host_and_responder_sessions();
        let wrong_kind = RemoteMessage {
            message_id: "resp-1".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::SignResponse(Response {
                responding_to: "request-1".to_string(),
                payload: Err("nope".to_string()),
            })),
        };
        let statement = build_outgoing_request_statement(
            &responder,
            "resp-statement".to_string(),
            vec![wrong_kind],
            fresh_statement_expiry(),
        )
        .unwrap();

        let err = wait_for_subtree(&host, vec![peer_page(statement)]).unwrap_err();

        assert_eq!(
            err,
            SsoRemoteResponseError::Failure(
                "Unexpected SSO response for product_subtree: SignResponse".to_string()
            )
        );
    }

    /// Messages are read in wire order: a reply that arrives before the peer's
    /// `Disconnected` in the same statement is still delivered.
    #[test]
    fn a_matching_reply_earlier_in_a_batch_wins_over_a_later_disconnect() {
        let (host, responder) = sso_host_and_responder_sessions();
        let ack = build_signed_session_response_statement(
            &responder,
            "request-1".to_string(),
            0,
            fresh_statement_expiry(),
        )
        .unwrap();
        let disconnect = RemoteMessage {
            message_id: "bye".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };
        let statement = build_outgoing_request_statement(
            &responder,
            "resp-statement".to_string(),
            vec![subtree_response("request-1"), disconnect],
            fresh_statement_expiry(),
        )
        .unwrap();

        let response = wait_for_subtree(&host, vec![peer_page(ack), peer_page(statement)]).unwrap();

        assert_eq!(
            response,
            Response {
                responding_to: "request-1".to_string(),
                payload: Ok([7; 32]),
            }
        );
    }

    /// A message that does not decode, such as a stale reply from an older peer
    /// left on the session channel, is skipped wherever it sits, so it cannot
    /// fail a wait for another request.
    #[test]
    fn an_undecodable_message_is_skipped() {
        let (host, responder) = sso_host_and_responder_sessions();
        let ack = build_signed_session_response_statement(
            &responder,
            "request-1".to_string(),
            0,
            fresh_statement_expiry(),
        )
        .unwrap();
        let statement_with = |data: Vec<Vec<u8>>| {
            let encrypted = encrypt_session_statement_data(
                &responder,
                &SsoStatementData::Request {
                    request_id: "resp-statement".to_string(),
                    data,
                },
            )
            .unwrap();
            build_signed_session_request_statement(&responder, encrypted, fresh_statement_expiry())
                .unwrap()
        };
        let garbage = vec![0xff, 0xff, 0xff];
        let reply = subtree_response("request-1").encode();

        let after = wait_for_subtree(
            &host,
            vec![
                peer_page(ack.clone()),
                peer_page(statement_with(vec![reply.clone(), garbage.clone()])),
            ],
        );
        let before = wait_for_subtree(
            &host,
            vec![
                peer_page(ack),
                peer_page(statement_with(vec![garbage, reply])),
            ],
        );

        let expected = Ok(Response {
            responding_to: "request-1".to_string(),
            payload: Ok([7; 32]),
        });
        assert_eq!((before, after), (expected.clone(), expected));
    }
}
