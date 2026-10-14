import Foundation
import StateMachine

/// Factory for creating configured state machines for a given payment.
protocol ExternalPaymentStateMachineCreating {
    func createStateMachine(
        for paymentId: String,
        store: ExternalPaymentStoring,
        context: DenominationBreakdownContext
    ) async throws -> StateMachine<ExternalPaymentStateFactory, ExternalPayment>
}
