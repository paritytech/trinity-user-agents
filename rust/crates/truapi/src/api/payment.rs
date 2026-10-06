//! Unified [`Payment`] trait.

use crate::versioned::payment::{
    HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
    HostPaymentBalanceSubscribeRequest, HostPaymentError, HostPaymentRequest, HostPaymentResponse,
    HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse, HostPaymentTopUpStatusSubscribeError,
    HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeRequest,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// Payment request and balance/status subscription methods.
#[wire_trait(id = 9)]
#[crate::async_trait]
pub trait Payment: Send + Sync {
    /// Subscribe to payment balance updates.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const balance = await firstValueFrom(
    ///   from(truapi.payment.balanceSubscribe({ request: {} })),
    /// );
    /// console.log("balance received:", balance);
    /// ```
    #[wire(id = 0)]
    async fn balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceSubscribeItem, CallError<HostPaymentBalanceSubscribeError>>
    {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Request a payment from the user.
    ///
    /// ```ts
    /// // Fund the balance first so the request is not rejected for lack of funds.
    /// const topUp = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Index", value: 0 } } },
    ///   id: "0x0000000000000000000000000000000000000000000000000000000000000002",
    /// });
    /// assert(topUp.isOk(), "topUp failed:", topUp);
    ///
    /// const result = await truapi.payment.request({
    ///   amount: 1000n,
    ///   destination:
    ///     "0x0000000000000000000000000000000000000000000000000000000000000000",
    /// });
    /// assert(result.isOk(), "request failed:", result);
    /// console.log("payment requested:", result.value);
    /// ```
    #[wire(id = 2)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequest,
    ) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// // Fund the balance and start a payment first so there is a status to watch.
    /// const topUp = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Index", value: 0 } } },
    ///   id: "0x0000000000000000000000000000000000000000000000000000000000000003",
    /// });
    /// assert(topUp.isOk(), "topUp failed:", topUp);
    ///
    /// const requested = await truapi.payment.request({
    ///   amount: 1000n,
    ///   destination:
    ///     "0x0000000000000000000000000000000000000000000000000000000000000000",
    /// });
    /// assert(requested.isOk(), "request failed:", requested);
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.payment.statusSubscribe({
    ///       request: { paymentId: requested.value.id },
    ///     }),
    ///   ),
    /// );
    /// console.log("payment status received:", status);
    /// ```
    #[wire(id = 3)]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>>
    {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Top up the user's payment balance, followed by `id` through
    /// `topUpStatusSubscribe`.
    ///
    /// ```ts
    /// const result = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Index", value: 0 } } },
    ///   id: "0x0000000000000000000000000000000000000000000000000000000000000001",
    /// });
    /// assert(result.isOk(), "topUp failed:", result);
    /// console.log("balance topped up");
    /// ```
    #[wire(id = 1)]
    async fn top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }

    /// Follow one top-up to its end.
    ///
    /// Emits the current status first, so a caller that reloads re-attaches.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.payment.topUpStatusSubscribe({
    ///       request: { id: "0x0000000000000000000000000000000000000000000000000000000000000001" },
    ///     }),
    ///   ),
    /// );
    /// console.log("top-up status:", status);
    /// ```
    #[wire(id = 4)]
    async fn top_up_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpStatusSubscribeRequest,
    ) -> Subscription<
        HostPaymentTopUpStatusSubscribeItem,
        CallError<HostPaymentTopUpStatusSubscribeError>,
    > {
        Subscription::interrupted(CallError::unavailable())
    }
}
