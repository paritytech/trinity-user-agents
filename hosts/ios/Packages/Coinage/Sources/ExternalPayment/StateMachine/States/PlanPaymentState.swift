import Foundation
import StateMachine

/// Invokes the planner and decides the next state. Every outcome is a verdict: an unreachable amount
/// and any thrown error both persist `failed`.
struct PlanPaymentState: StateMachineState {
    typealias StateFactory = ExternalPaymentStateFactory
    typealias PersistentValue = ExternalPayment

    let payment: ExternalPayment
    let isTerminal = false

    func transit(
        with factory: ExternalPaymentStateFactory
    ) async -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        do {
            let preview = try await factory.planner.plan(amount: payment.amountInPlanks, context: factory.context)
            factory.logger?
                .debug(
                    "Payment \(payment.identifier) planned: \(preview.vouchers.count) vouchers, \(preview.coins.count) coins"
                )

            switch preview {
            case let .unloadVouchers(offboarding):
                return factory.makeOffboardVouchersState(payment: payment, offboarding: offboarding)
            case let .loadCoins(coins, exactVouchers):
                return factory.makeOnboardCoinsState(
                    payment: payment,
                    coins: coins.map(\.coin),
                    exactVouchers: exactVouchers.map(\.voucher)
                )
            case .notEnoughBalance:
                return factory.makeFailedState(payment: payment, reason: "insufficient balance")
            }
        } catch {
            return factory.makeFailedState(payment: payment, reason: error.localizedDescription)
        }
    }

    func memo() async -> ExternalPayment {
        var currentPayment = payment
        currentPayment.stage = .plan
        currentPayment.plannedVoucherIndices = []
        currentPayment.surplusInPlanks = 0
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}
