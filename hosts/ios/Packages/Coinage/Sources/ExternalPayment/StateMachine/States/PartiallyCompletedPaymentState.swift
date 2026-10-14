import Foundation
import StateMachine

/// Terminal state indicating some — but not all — of the payment's unload transactions executed.
/// Not a failure: money did move, just not the whole amount.
struct PartiallyCompletedPaymentState: StateMachineState {
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
        currentPayment.stage = .partiallyCompleted
        currentPayment.failureReason = reason
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}
