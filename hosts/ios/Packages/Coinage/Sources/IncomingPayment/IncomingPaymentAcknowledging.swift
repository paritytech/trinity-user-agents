import Foundation
import SubstrateSdk

/// Surfaces a top-up's unhappy ending to the user, from the durable operation rather than the host
/// call that asked for it (`paymentTopUp` has long since returned by the time there is a verdict).
///
/// Only the two unhappy terminal outcomes are worth a prompt — a top-up that simply worked says
/// nothing — so the service calls this only for `.claimedPartially` / `.notClaimed`. Implemented
/// app-side (routes to the top-up acknowledgement screens). `requestedAmount` is the top-up's
/// original amount, shown against the credited figure on the mismatch screen.
public protocol IncomingPaymentAcknowledging: Sendable {
    func acknowledge(
        productId: String,
        paymentId: IncomingPaymentId,
        requestedAmount: Balance,
        outcome: IncomingPaymentTerminalOutcome
    ) async
}
