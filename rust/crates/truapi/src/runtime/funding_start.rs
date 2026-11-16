//! Funds a funding provider starts for a session it serves: a top-up that
//! credits the user, or a payment request that collects from them.
//!
//! Each is recorded on its session before the host's engine is asked, and
//! settled by what the engine did, so a provider's later report can name it.
//! A provider's worker reaches this through `payment.topUp` and
//! `payment.request`; the core's native route calls it directly.

use std::sync::Arc;

use parity_scale_codec::Encode;
use truapi::{CallError, latest};

use super::{
	RuntimeServices,
	funding::{Claim, FundsAsked, engine_holds},
};
use crate::{
	current_unix_millis,
	host_logic::funding::StartRecord,
	platform::{ExecutableKind, ProductContext},
};

/// Funds a provider is starting for a session it serves, recorded before
/// the host is asked.
pub(crate) struct FundingStart {
	intent: String,
	host_id: [u8; 32],
	/// Whether this call recorded them, rather than a call before it.
	fresh: bool,
	/// Keeps a second call under the id out until this one finished.
	_guard: Claim<[u8; 32]>,
}

/// What the host's engine did with funds a provider started for a session.
pub(crate) enum EngineAnswer {
	/// It took them.
	Took,
	/// It already held funds under the id.
	Holds,
	/// It refused them, so nothing moves under the id.
	Refused,
	/// It failed without saying whether it took them.
	Unsure,
}

impl RuntimeServices {
	/// Record funds `provider` starts for funding session `intent`, before
	/// the host is asked, so its reports can name them once the host takes
	/// them. `not_served` when `provider` is no funding provider's worker or
	/// does not serve that session in the funds' direction, or the session
	/// holds as many funds as it may; `already_exists` when the id is held
	/// with other terms, by another session, or by the host's engine already.
	pub(crate) async fn begin_funding_start<E>(
		self: &Arc<Self>,
		provider: &ProductContext,
		intent: String,
		asked: FundsAsked,
		already_exists: E,
		not_served: E,
	) -> Result<FundingStart, CallError<E>> {
		let host_id = asked.host_id;
		let registry = self.funding();
		if provider.execution_kind != ExecutableKind::Worker || registry.platform().is_none() {
			return Err(CallError::Domain(not_served));
		}
		let Some(guard) = registry.begin_start(host_id) else {
			return Err(CallError::Domain(already_exists));
		};
		// A first call under the id must not take over funds the engine
		// holds already, which this session never recorded.
		if !registry.recorded_start(&intent, &host_id) {
			let held = engine_holds(self, provider, asked.direction, host_id)
				.await
				.map_err(|reason| CallError::HostFailure { reason })?;
			if held {
				return Err(CallError::Domain(already_exists));
			}
		}
		let recorded = registry
			.record_start(current_unix_millis(), &provider.product_id, &intent, asked)
			.await?;
		match recorded {
			None | Some(StartRecord::Full) => Err(CallError::Domain(not_served)),
			Some(StartRecord::Conflict) => Err(CallError::Domain(already_exists)),
			Some(kind @ (StartRecord::New | StartRecord::Repeat)) => {
				Ok(FundingStart { intent, host_id, fresh: kind == StartRecord::New, _guard: guard })
			},
		}
	}

	/// Settle the record `start` made by what the engine did: kept as taken
	/// when it took the funds, or when it holds the id from an earlier call
	/// that recorded them; dropped when it refused them. A record the engine
	/// left unsure, or that cannot be stored, is placed later by the
	/// reconciler, from the engine.
	pub(crate) async fn finish_funding_start(&self, start: FundingStart, answer: EngineAnswer) {
		let taken = match answer {
			EngineAnswer::Took => true,
			EngineAnswer::Holds => !start.fresh,
			EngineAnswer::Refused => false,
			EngineAnswer::Unsure => {
				drop(start);
				self.funding().wake();
				return;
			},
		};
		let finished =
			self.funding().finish_start(current_unix_millis(), &start.intent, start.host_id, taken).await;
		if let Err(error) = finished {
			tracing::warn!(%error, "storing what the host did with started funds failed");
		}
	}

	/// Ask the host to credit `request`, which `provider` starts for funding
	/// session `intent`. `request.id` is the id the host receives, already
	/// derived for `provider`.
	pub(crate) async fn start_funding_top_up(
		self: &Arc<Self>,
		provider: &ProductContext,
		intent: String,
		request: latest::HostPaymentTopUpRequest,
	) -> Result<(), CallError<latest::HostPaymentTopUpError>> {
		let platform = self.top_up_platform().ok_or(CallError::Unsupported)?;
		// A hash, so the core keeps no source key.
		let terms = sp_crypto_hashing::blake2_256(
			&(b"truapi/funding-terms", request.into, &request.source).encode(),
		);
		let start = self
			.begin_funding_start(
				provider,
				intent,
				FundsAsked {
					direction: latest::FundingDirection::In,
					host_id: request.id,
					amount: request.amount,
					terms,
				},
				latest::HostPaymentTopUpError::AlreadyExists,
				latest::HostPaymentTopUpError::FundingNotServed,
			)
			.await?;
		let started = platform.top_up(provider, request).await;
		let answer = match &started {
			Ok(()) => EngineAnswer::Took,
			Err(latest::HostPaymentTopUpError::AlreadyExists) => EngineAnswer::Holds,
			Err(latest::HostPaymentTopUpError::Unknown { .. }) => EngineAnswer::Unsure,
			Err(_) => EngineAnswer::Refused,
		};
		self.finish_funding_start(start, answer).await;
		started.map_err(CallError::Domain)
	}

	/// Ask the host to collect `request`, which `provider` starts for funding
	/// session `intent`. `request.id` is the id the host receives, already
	/// derived for `provider`.
	pub(crate) async fn start_funding_payment(
		self: &Arc<Self>,
		provider: &ProductContext,
		intent: String,
		request: latest::HostPaymentRequest,
	) -> Result<(), CallError<latest::HostPaymentError>> {
		let platform = self.payment_platform().ok_or(CallError::Unsupported)?;
		let terms = sp_crypto_hashing::blake2_256(
			&(b"truapi/funding-terms", request.from, request.destination).encode(),
		);
		let start = self
			.begin_funding_start(
				provider,
				intent,
				FundsAsked {
					direction: latest::FundingDirection::Out,
					host_id: request.id,
					amount: request.amount,
					terms,
				},
				latest::HostPaymentError::AlreadyExists,
				latest::HostPaymentError::FundingNotServed,
			)
			.await?;
		let requested = platform.request_payment(provider, request).await;
		let answer = match &requested {
			Ok(()) => EngineAnswer::Took,
			Err(latest::HostPaymentError::AlreadyExists) => EngineAnswer::Holds,
			Err(latest::HostPaymentError::Unknown { .. }) => EngineAnswer::Unsure,
			Err(_) => EngineAnswer::Refused,
		};
		self.finish_funding_start(start, answer).await;
		requested.map_err(CallError::Domain)
	}
}
