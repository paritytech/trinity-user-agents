//! Unified [`FundingProvider`] trait.

use crate::versioned::funding_provider::{
    HostFundingAnswerQuoteError, HostFundingAnswerQuoteRequest, HostFundingAnswerQuoteResponse,
    HostFundingPresentFrameError, HostFundingPresentFrameRequest, HostFundingPresentFrameResponse,
    HostFundingReportError, HostFundingReportRequest, HostFundingReportResponse,
    HostFundingSaveError, HostFundingSaveRequest, HostFundingSaveResponse,
    HostFundingServeSubscribeError, HostFundingServeSubscribeItem,
    HostFundingServeSubscribeRequest,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// Serve the funding sessions the user chose this provider for.
///
/// The provider's worker runs each session it is assigned and reports its
/// progress. The host stores every report, and decides `Delivered` and
/// `Released` itself from the top-ups and payments the provider names.
#[wire_trait(id = 23)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait FundingProvider: Send + Sync {
    /// Receive the sessions assigned to the calling provider, and requests to
    /// cancel them.
    ///
    /// Emits every session still in flight first, so a restarted worker picks
    /// up where it stopped, then each new assignment and cancel request.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(
    ///   from(truapi.fundingProvider.serveSubscribe()),
    /// );
    /// console.log("funding provider item:", item);
    /// ```
    #[wire(id = 0)]
    async fn serve_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostFundingServeSubscribeRequest,
    ) -> Subscription<HostFundingServeSubscribeItem, CallError<HostFundingServeSubscribeError>>
    {
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
    #[wire(id = 1)]
    async fn report(
        &self,
        _cx: &CallContext,
        _request: HostFundingReportRequest,
    ) -> Result<HostFundingReportResponse, CallError<HostFundingReportError>> {
        Err(CallError::unavailable())
    }

    /// Show one of the provider's screens, such as KYC or card entry, in a
    /// frame the host owns. Answers once it closes.
    ///
    /// ```ts
    /// const result = await truapi.fundingProvider.presentFrame({
    ///   intent: "fs_example",
    ///   route: "/kyc",
    /// });
    /// console.log("provider frame:", result);
    /// ```
    #[wire(id = 2)]
    async fn present_frame(
        &self,
        _cx: &CallContext,
        _request: HostFundingPresentFrameRequest,
    ) -> Result<HostFundingPresentFrameResponse, CallError<HostFundingPresentFrameError>> {
        Err(CallError::unavailable())
    }

    /// Answer a `Quote` the host sent on `serveSubscribe`, with the
    /// provider's price or why it will not give one. The worker calls its own
    /// API for it, through the onramp adapter when that needs the provider's
    /// key.
    ///
    /// ```ts
    /// const result = await truapi.fundingProvider.answerQuote({
    ///   askId: "qa_example",
    ///   answer: { tag: "Refused", value: { reason: { tag: "Unavailable", value: undefined } } },
    /// });
    /// console.log("quote answered:", result);
    /// ```
    #[wire(id = 3)]
    async fn answer_quote(
        &self,
        _cx: &CallContext,
        _request: HostFundingAnswerQuoteRequest,
    ) -> Result<HostFundingAnswerQuoteResponse, CallError<HostFundingAnswerQuoteError>> {
        Err(CallError::unavailable())
    }

    /// Save the provider's own state for a session it serves, such as its
    /// order id, so a restarted worker can carry on. The host keeps it with
    /// the session, hands it back in `Assigned`, and drops it once the
    /// session ends.
    ///
    /// ```ts
    /// const result = await truapi.fundingProvider.save({
    ///   intent: "fs_example",
    ///   state: "0x6f5f31",
    /// });
    /// console.log("state saved:", result);
    /// ```
    #[wire(id = 4)]
    async fn save(
        &self,
        _cx: &CallContext,
        _request: HostFundingSaveRequest,
    ) -> Result<HostFundingSaveResponse, CallError<HostFundingSaveError>> {
        Err(CallError::unavailable())
    }
}
