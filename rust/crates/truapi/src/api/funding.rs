//! Unified [`Funding`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::funding::{
		HostFundingError, HostFundingRequest, HostFundingResponse, HostFundingStatusSubscribeError,
		HostFundingStatusSubscribeItem, HostFundingStatusSubscribeRequest,
	},
	wire, wire_trait,
};

/// Move value into or out of the user's balance and watch it to completion.
///
/// The host draws the whole flow. The core decides `Delivered` and `Released`
/// itself, from the funds that moved, never from the host's or a provider's
/// word.
#[wire_trait(id = 22)]
#[crate::async_trait]
pub trait Funding: Send + Sync {
	/// Open the funding modality for a direction and, optionally, an amount,
	/// under a caller-chosen `id` the session is then watched by. Returns once
	/// the user starts the session.
	///
	/// ```ts
	/// const result = await truapi.funding.request({
	///   id: "0x0000000000000000000000000000000000000000000000000000000000000001",
	///   direction: "In",
	///   amount: 1000n,
	/// });
	/// assert(result.isOk(), "funding.request failed:", result);
	/// console.log("funding session opened");
	/// ```
	#[wire(id = 0)]
	async fn request(
		&self,
		_cx: &CallContext,
		_request: HostFundingRequest,
	) -> Result<HostFundingResponse, CallError<HostFundingError>> {
		Err(CallError::unavailable())
	}

	/// Watch one of the caller's funding sessions to completion.
	///
	/// Emits the current stage, later stages, then exactly one terminal item.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const status = await firstValueFrom(
	///   from(
	///     truapi.funding.statusSubscribe({
	///       request: {
	///         id: "0x0000000000000000000000000000000000000000000000000000000000000001",
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("funding status:", status);
	/// ```
	#[wire(id = 1)]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostFundingStatusSubscribeRequest,
	) -> Subscription<HostFundingStatusSubscribeItem, CallError<HostFundingStatusSubscribeError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
