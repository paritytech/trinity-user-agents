import Foundation
import StateMachine

/// Terminal state indicating the payment completed successfully.
struct CompletedPaymentState: StateMachineState {
    typealias StateFactory = ExternalPaymentStateFactory
    typealias PersistentValue = ExternalPayment

    let payment: ExternalPayment
    let isTerminal = true

    func transit(
        with _: ExternalPaymentStateFactory
    ) async -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        AnyStateMachineState(self)
    }

    func memo() async -> ExternalPayment {
        var currentPayment = payment
        currentPayment.stage = .completed
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}
