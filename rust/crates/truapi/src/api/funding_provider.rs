//! Unified [`FundingProvider`] trait.

use crate::{
	CallContext, CallError,
	versioned::funding_provider::{
		HostFundingCloseFrameError, HostFundingCloseFrameRequest, HostFundingCloseFrameResponse,
		HostFundingPresentFrameError, HostFundingPresentFrameRequest,
		HostFundingPresentFrameResponse, HostFundingReportError, HostFundingReportRequest,
		HostFundingReportResponse, ProductFundingAssignError, ProductFundingAssignRequest,
		ProductFundingAssignResponse, ProductFundingCancelError, ProductFundingCancelRequest,
		ProductFundingCancelResponse, ProductFundingQuoteError, ProductFundingQuoteRequest,
		ProductFundingQuoteResponse, ProductFundingSettleCancelError,
		ProductFundingSettleCancelRequest, ProductFundingSettleCancelResponse,
	},
	wire, wire_trait,
};

/// Serve the funding sessions the user chooses this provider for.
///
/// The core calls the provider's worker to hand it sessions and cancels, and
/// awaits one answer to each. The worker calls the core to report progress. The core decides
/// `Delivered` and `Released` itself, from the top-ups and payments the worker names.
///
/// When the worker connects, the core assigns it every session it still
/// serves, then repeats any cancel or cancel settlement it never answered.
/// The worker keeps its own state per session, such as an onramp order id, in
/// its storage under the session's `intent`.
#[wire_trait(id = 24)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait FundingProvider: Send + Sync {
	/// Price an ask for the host's quote list, or say why the provider will
	/// not. The core asks every provider and drops a call that has not
	/// answered within 10 seconds.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.fundingProvider.onQuote(({ ask }) => {
	///   console.log("asked to price:", ask.amount);
	///   return ok({ tag: "Refused", value: { reason: { tag: "Unavailable", value: undefined } } });
	/// });
	/// ```
	#[wire(host_initiated, id = 0)]
	async fn quote(
		&self,
		_cx: &CallContext,
		_request: ProductFundingQuoteRequest,
	) -> Result<ProductFundingQuoteResponse, CallError<ProductFundingQuoteError>> {
		Err(CallError::unavailable())
	}

	/// Take a session the user chose this provider for, or resume one after
	/// the worker restarted: `last_update` says where it stopped. The worker
	/// answers once it holds the session.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.fundingProvider.onAssign(({ session }) => {
	///   console.log("assigned:", session.intent);
	///   return ok(undefined);
	/// });
	/// ```
	#[wire(host_initiated, id = 1)]
	async fn assign(
		&self,
		_cx: &CallContext,
		_request: ProductFundingAssignRequest,
	) -> Result<ProductFundingAssignResponse, CallError<ProductFundingAssignError>> {
		Err(CallError::unavailable())
	}

	/// Stop a session the user wants to cancel. Before money moved the
	/// provider answers `Stopped`; after, `Offer` or `Refused`.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.fundingProvider.onCancel(({ intent }) => {
	///   console.log("cancel:", intent);
	///   return ok({ tag: "Stopped", value: undefined });
	/// });
	/// ```
	#[wire(host_initiated, id = 2)]
	async fn cancel(
		&self,
		_cx: &CallContext,
		_request: ProductFundingCancelRequest,
	) -> Result<ProductFundingCancelResponse, CallError<ProductFundingCancelError>> {
		Err(CallError::unavailable())
	}

	/// Hear whether the user accepted the provider's cancel offer. On
	/// `Accepted` the provider returns the refund and reports `Failed`; on
	/// `Declined` it carries on. The worker answers once it has the
	/// decision.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.fundingProvider.onSettleCancel(({ intent, decision }) => {
	///   console.log("cancel decision:", intent, decision.tag);
	///   return ok(undefined);
	/// });
	/// ```
	#[wire(host_initiated, id = 3)]
	async fn settle_cancel(
		&self,
		_cx: &CallContext,
		_request: ProductFundingSettleCancelRequest,
	) -> Result<ProductFundingSettleCancelResponse, CallError<ProductFundingSettleCancelError>> {
		Err(CallError::unavailable())
	}

	/// Report progress on a session assigned to the calling provider.
	///
	/// The host stores the update before answering.
	///
	/// ```ts
	/// const result = await truapi.fundingProvider.report({
	///   intent: "fs_example",
	///   update: { tag: "AwaitingPayment", value: undefined },
	/// });
	/// console.log("funding report:", result);
	/// ```
	#[wire(id = 4)]
	async fn report(
		&self,
		_cx: &CallContext,
		_request: HostFundingReportRequest,
	) -> Result<HostFundingReportResponse, CallError<HostFundingReportError>> {
		Err(CallError::unavailable())
	}

	/// Show one of the provider's screens, such as KYC or card entry, or an
	/// external https page such as an onramp's widget, in a frame the host
	/// owns. Answers once it closes.
	///
	/// ```ts
	/// const result = await truapi.fundingProvider.presentFrame({
	///   intent: "fs_example",
	///   target: { tag: "App", value: { route: "/kyc" } },
	/// });
	/// console.log("provider frame:", result);
	/// ```
	#[wire(id = 5)]
	async fn present_frame(
		&self,
		_cx: &CallContext,
		_request: HostFundingPresentFrameRequest,
	) -> Result<HostFundingPresentFrameResponse, CallError<HostFundingPresentFrameError>> {
		Err(CallError::unavailable())
	}

	/// Close the screen `present_frame` opened for a session once the
	/// provider is done with it, such as a card payment the user made. That
	/// `present_frame` call answers `Closed`, and the host shows the
	/// session's progress in its place. Closing a session with no screen
	/// open does nothing.
	///
	/// ```ts
	/// const result = await truapi.fundingProvider.closeFrame({
	///   intent: "fs_example",
	/// });
	/// console.log("provider frame closed:", result);
	/// ```
	#[wire(id = 6)]
	async fn close_frame(
		&self,
		_cx: &CallContext,
		_request: HostFundingCloseFrameRequest,
	) -> Result<HostFundingCloseFrameResponse, CallError<HostFundingCloseFrameError>> {
		Err(CallError::unavailable())
	}
}
