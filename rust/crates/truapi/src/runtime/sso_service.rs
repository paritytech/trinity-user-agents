//! Context and reply types shared by SSO handlers and generated dispatch.

use core::fmt::Display;
use std::{
	collections::{HashMap, VecDeque},
	sync::{Mutex, PoisonError},
};

use truapi::{CallContext, CancellationToken};

use super::authority::AuthoritySession;
use crate::host_internal::{
	sso_messages::{RemoteMessage, RemoteMessageData, Response, v1},
	sso_wire::ResponseOutcome,
};

/// Per-request context handed to every service method.
pub struct SsoRequestContext {
	/// Call context correlated to the request's `message_id`.
	pub call: CallContext,
	/// Signing session resolved once for the request.
	pub session: AuthoritySession,
}

impl SsoRequestContext {
	/// Context for the request sent as `message_id`, which the pairing host
	/// withdraws by firing `cancel`.
	pub fn new(message_id: &str, session: AuthoritySession, cancel: CancellationToken) -> Self {
		Self { call: CallContext::with_parts(String::from(message_id), cancel), session }
	}
}

/// How many withdrawals naming a request not yet seen are remembered.
const MAX_EARLY_WITHDRAWALS: usize = 64;

/// Requests the pairing host can still withdraw, by `message_id`, and the
/// withdrawals that arrived before the request they name.
#[derive(Default)]
pub struct SsoWithdrawals {
	state: Mutex<WithdrawalState>,
}

#[derive(Default)]
struct WithdrawalState {
	running: HashMap<String, CancellationToken>,
	early: VecDeque<String>,
}

/// A request registered with [`SsoWithdrawals`] until it is dropped.
pub struct Withdrawable<'a> {
	withdrawals: &'a SsoWithdrawals,
	message_id: String,
	/// Fired when the pairing host withdraws the request.
	pub cancel: CancellationToken,
}

impl Drop for Withdrawable<'_> {
	fn drop(&mut self) {
		// A panic here while already unwinding would abort the process.
		let mut state = self.withdrawals.state.lock().unwrap_or_else(PoisonError::into_inner);
		state.running.remove(&self.message_id);
	}
}

impl SsoWithdrawals {
	fn lock(&self) -> std::sync::MutexGuard<'_, WithdrawalState> {
		self.state.lock().expect("SSO withdrawals mutex poisoned")
	}

	/// Register the request sent as `message_id`, or `None` when the pairing
	/// host withdrew it before it arrived.
	pub fn begin(&self, message_id: &str) -> Option<Withdrawable<'_>> {
		let mut state = self.lock();
		if let Some(position) = state.early.iter().position(|id| id == message_id) {
			state.early.remove(position);
			return None;
		}
		let cancel = state.running.entry(message_id.to_string()).or_default().clone();
		Some(Withdrawable { withdrawals: self, message_id: message_id.to_string(), cancel })
	}

	/// Withdraw the request sent as `message_id`: fire its token if it is
	/// running, or remember it so it never starts.
	pub fn withdraw(&self, message_id: &str) {
		let mut state = self.lock();
		if let Some(cancel) = state.running.get(message_id).cloned() {
			drop(state);
			cancel.cancel();
			return;
		}
		if state.early.iter().any(|id| id == message_id) {
			return;
		}
		if state.early.len() == MAX_EARLY_WITHDRAWALS &&
			let Some(forgotten) = state.early.pop_front()
		{
			tracing::warn!(
				message_id = %forgotten,
				"forgot an early SSO withdrawal; that request will be served in full"
			);
		}
		state.early.push_back(message_id.to_string());
	}
}

/// What the service dispatcher produced for one wire message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
	/// Response to post back, with its transcript outcome.
	Response(Box<Answer>),
	/// The peer ended the session.
	Disconnected,
	/// The pairing host withdrew the request sent as this `message_id`.
	Withdraw(String),
	/// The request was withdrawn, so it has no response to post.
	Withdrawn,
	/// A response variant arrived where only requests are expected.
	NotARequest(&'static str),
}

/// A served request: the response envelope and how the transcript reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
	/// Response envelope, `message_id` suffixed with `:response`.
	pub message: RemoteMessage,
	/// Transcript classification of the payload.
	pub outcome: ResponseOutcome,
}

/// A handler's payload and optional transcript outcome, before wire wrapping.
pub struct SsoReply<P> {
	payload: P,
	outcome: Option<ResponseOutcome>,
}

impl<P> From<P> for SsoReply<P> {
	fn from(payload: P) -> Self {
		Self { payload, outcome: None }
	}
}

impl<P> SsoReply<P> {
	/// Supply a transcript outcome when the payload alone does not describe the result.
	pub fn with_outcome(mut self, outcome: ResponseOutcome) -> Self {
		self.outcome = Some(outcome);
		self
	}
}

impl<T, E: Display> SsoReply<Result<T, E>> {
	/// Address the reply and wrap it in the response variant selected by the request.
	pub fn finish(
		self,
		message_id: &str,
		wrap: impl FnOnce(Response<Result<T, E>>) -> v1::RemoteMessage,
	) -> Answer {
		let outcome = self.outcome.unwrap_or_else(|| ResponseOutcome::from_payload(&self.payload));
		Answer {
			message: RemoteMessage {
				message_id: format!("{message_id}:response"),
				data: RemoteMessageData::V1(wrap(Response {
					responding_to: message_id.to_string(),
					payload: self.payload,
				})),
			},
			outcome,
		}
	}
}

#[cfg(test)]
mod tests;
