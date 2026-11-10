//! SSO remote messaging over the people-chain statement store: submits an
//! encrypted request statement to the paired signing host and waits for the
//! matching response, honoring timeouts and local/peer disconnect signals.

use core::mem;
use std::{
	fmt::{self, Display},
	sync::Mutex,
};

use super::statement_store_rpc;
use crate::{
	chain_runtime::{retry_while_halted, says_chain_halted},
	host_internal::{
		sso_messages::{Response, SsoSessionStatement, decode_sso_session_statement, v1},
		sso_wire::SsoRequest,
	},
	host_logic::{session::SsoSessionInfo, statement_store::parse_new_statements_result},
	time::RealTimer,
	unix_time::current_unix_secs,
};

use futures::{
	FutureExt, StreamExt, channel::oneshot, future::BoxFuture, pin_mut, stream::BoxStream,
};
use serde_json::Value;
use subxt_rpcs::{RpcClient, client::RpcSubscription};
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SsoSessionKey {
	own: [u8; 32],
	peer: [u8; 32],
}

impl SsoSessionKey {
	/// Key for the session's own/peer id pair.
	pub fn from_session(session: &SsoSessionInfo) -> Self {
		Self { own: session.session_id_own, peer: session.session_id_peer }
	}
	/// Whether this channel is still selected by the canonical session.
	pub fn matches(self, state: &crate::host_logic::session::SessionState) -> bool {
		state.current().as_ref().is_some_and(|current| {
			current.sso.as_ref().is_some_and(|sso| Self::from_session(sso) == self)
		})
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
		let mut inner = self.inner.lock().expect("session disconnect mutex poisoned");
		inner.next_id = inner.next_id.wrapping_add(1);
		let id = inner.next_id;
		inner.waiters.push((id, SsoSessionKey::from_session(session), tx));
		(SessionDisconnectGuard { disconnects: self.clone(), id }, rx)
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
			let mut inner = self.inner.lock().expect("session disconnect mutex poisoned");
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
	/// The request sent as `remote_message_id`, cancelled for `reason`.
	pub fn new(reason: CancellationReason, remote_message_id: &str) -> Self {
		Self { reason, remote_message_id: remote_message_id.to_string() }
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
		Self { reason: self.reason, remote_message_id: remote_message_id.to_string() }
	}
}

impl Display for CancelError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self.reason {
			CancellationReason::Cancelled => {
				write!(f, "{SSO_CALL_CANCELLED_REASON} for {}", self.remote_message_id)
			},
			reason @ CancellationReason::TimedOut { .. } => {
				write!(f, "SSO response {reason} for {}", self.remote_message_id)
			},
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
	/// The request never reached the signing host because the chain stayed
	/// halted or reached no peer. A later call can succeed.
	#[display("{_0}")]
	ChainUnavailable(String),
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
			Some(rx) => match rx.await {
				Ok(reason) => disconnect_error(reason),
				Err(_) => SsoRemoteResponseError::LocalDisconnected,
			},
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
		match decode_sso_session_statement(session, &statement, statement_request_id)? {
			Some(SsoSessionStatement::RequestAccepted) => {
				*request_accepted = true;
				if let Some(response) = pending_remote_response.take() {
					return Ok(Some(response));
				}
			},
			Some(SsoSessionStatement::RemoteMessages(messages)) => {
				for message in messages {
					let message = match message {
						Ok(message) => message,
						Err(error) => {
							debug!(%error, "skipping undecodable SSO remote message");
							continue;
						},
					};
					if message == v1::RemoteMessage::Disconnected {
						return Err(SsoRemoteResponseError::PeerDisconnected);
					}
					let Some(matched) = matches(message) else {
						continue;
					};
					let response = matched?;
					if *request_accepted {
						return Ok(Some(response));
					}
					*pending_remote_response = Some(response);
					break;
				}
			},
			None => {},
		}
	}
	Ok(None)
}

/// Live statement-store subscription for a single topic, waiting out a chain
/// halt unless `cancel` fires first.
pub async fn subscribe_statement_topic(
	rpc_client: &RpcClient,
	topic: [u8; 32],
	cancel: &CancellationToken,
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
	let topics = [topic];
	let subscribe = || statement_store_rpc::subscribe_match_all(rpc_client, &topics);
	retry_while_halted(subscribe, says_chain_halted, &RealTimer, cancel).await
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
	let mut last = LAST_SSO_STATEMENT_EXPIRY.lock().expect("SSO statement expiry mutex poisoned");
	let expiry = next_statement_expiry(*last, expiry_floor);
	*last = expiry;
	expiry
}

fn next_statement_expiry(last: u64, expiry_floor: u64) -> u64 {
	expiry_floor.max(last.saturating_add(1))
}

#[cfg(test)]
mod tests;
