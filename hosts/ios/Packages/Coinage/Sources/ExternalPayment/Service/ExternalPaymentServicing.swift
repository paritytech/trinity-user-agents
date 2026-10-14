import AsyncExtensions
import Foundation
import SubstrateSdk

/// Public interface for previewing, initiating and monitoring external payments.
public protocol ExternalPaymentServicing {
    func previewPayment(
        for amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> ExternalPaymentPreview

    /// Whether private vouchers alone would pay `amount`; anything else gives up privacy the user
    /// should be asked about first.
    func canExecuteExternalPaymentPrivately(
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> Bool

    /// Registers a payment with stage `.plan`. Throws ``ExternalPaymentError/alreadyExists`` for a
    /// known `(productId, paymentId)` and ``ExternalPaymentError/invalidPaymentId`` for an empty id.
    func initiatePayment(
        productId: String,
        paymentId: String,
        amountInPlanks: Balance,
        destination: AccountId
    ) async throws

    /// Ends after the first terminal status. Throws ``ExternalPaymentError/notFound`` when nothing is
    /// registered under `(productId, paymentId)`.
    func subscribePaymentStatus(
        productId: String,
        paymentId: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus>

    func setup(with context: DenominationBreakdownContext)
}

public enum ExternalPaymentStatus: Sendable, Equatable {
    case processing
    case completed
    /// Terminal with a shortfall: `settledInPlanks` reached the destination, the rest never will.
    case partiallyCompleted(settledInPlanks: Balance)
    case failed(reason: String)
}
