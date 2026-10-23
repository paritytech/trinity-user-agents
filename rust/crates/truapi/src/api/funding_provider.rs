//! Unified [`FundingProvider`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::funding_provider::{
		HostFundingPresentFrameError, HostFundingPresentFrameRequest,
		HostFundingPresentFrameResponse, HostFundingReportError, HostFundingReportRequest,
		HostFundingReportResponse, ProductFundingAssignError, ProductFundingAssignItem,
		ProductFundingAssignRequest, ProductFundingCancelError, ProductFundingCancelItem,
		ProductFundingCancelRequest, ProductFundingSettleCancelError,
		ProductFundingSettleCancelItem, ProductFundingSettleCancelRequest,
	},
	wire, wire_trait,
};

/// Serve the funding sessions the user chooses this provider for.
///
/// The core calls the provider's worker to hand it sessions and cancels, and
/// each call takes one answer item. The worker calls the core to report
/// progress. The core decides `Delivered` and `Released` itself, from the
/// top-ups and payments the worker names.
///
/// When the worker connects, the core assigns it every session it still
/// serves, then repeats any cancel or cancel settlement it never answered.
/// The worker keeps its own state per session, such as an onramp order id, in
/// its storage under the session's `intent`.
#[wire_trait(id = 24)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait FundingProvider: Send + Sync {
	/// Take a session the user chose this provider for, or resume one after
	/// the worker restarted: `last_update` says where it stopped. The worker
	/// answers with one item once it holds the session.
	///
	/// ```ts
	/// truapi.fundingProvider.onAssign(({ session }, send, interrupt) => {
	///   console.log("assigned:", session.intent);
	///   send(undefined);
	///   interrupt();
	/// });
	/// ```
	#[wire(host_initiated, id = 1)]
	fn assign(
		&self,
		_cx: &CallContext,
		_request: ProductFundingAssignRequest,
	) -> Subscription<ProductFundingAssignItem, CallError<ProductFundingAssignError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Stop a session the user wants to cancel. Before money moved the
	/// provider answers `Stopped`; after, `Offer` or `Refused`.
	///
	/// ```ts
	/// truapi.fundingProvider.onCancel(({ intent }, send, interrupt) => {
	///   console.log("cancel:", intent);
	///   send({ tag: "Stopped", value: undefined });
	///   interrupt();
	/// });
	/// ```
	#[wire(host_initiated, id = 2)]
	fn cancel(
		&self,
		_cx: &CallContext,
		_request: ProductFundingCancelRequest,
	) -> Subscription<ProductFundingCancelItem, CallError<ProductFundingCancelError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Hear whether the user accepted the provider's cancel offer. On
	/// `Accepted` the provider returns the refund and reports `Failed`; on
	/// `Declined` it carries on. The worker answers with one item once it
	/// has the decision.
	///
	/// ```ts
	/// truapi.fundingProvider.onSettleCancel(({ intent, decision }, send, interrupt) => {
	///   console.log("cancel decision:", intent, decision.tag);
	///   send(undefined);
	///   interrupt();
	/// });
	/// ```
	#[wire(host_initiated, id = 3)]
	fn settle_cancel(
		&self,
		_cx: &CallContext,
		_request: ProductFundingSettleCancelRequest,
	) -> Subscription<ProductFundingSettleCancelItem, CallError<ProductFundingSettleCancelError>> {
		Subscription::interrupted(CallError::unavailable())
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
}
