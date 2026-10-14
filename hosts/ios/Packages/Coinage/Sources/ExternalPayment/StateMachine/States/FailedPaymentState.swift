import Foundation
import StateMachine

/// Terminal state indicating the payment failed permanently.
struct FailedPaymentState: StateMachineState {
    typealias StateFactory = ExternalPaymentStateFactory
    typealias PersistentValue = ExternalPayment

    let payment: ExternalPayment
    let reason: String
    let isTerminal = true

    func transit(
        with _: ExternalPaymentStateFactory
    ) async -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        AnyStateMachineState(self)
    }

    func memo() async -> ExternalPayment {
        var currentPayment = payment
        currentPayment.stage = .failed
        currentPayment.failureReason = reason
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}
