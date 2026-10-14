import Foundation
import StateMachine
import SubstrateSdk

/// Recycles the selected coins under the payment's own durability group and awaits that group's
/// outcome through the recycler. On `allRecycled` (best-block inclusion is enough) the planner picks
/// what to unload from the exact vouchers plus the recycled ones; `incomplete`, a shortfall and any
/// thrown error fail the payment — there are no retries.
///
/// The exact vouchers are persisted with the stage, so a relaunch (`coins` empty) re-joins the group
/// and continues with the same selection. A relaunch that finds no group registered fails:
/// re-planning could spend the coins twice if the submission did land.
struct OnboardCoinsPaymentState: StateMachineState {
    typealias StateFactory = ExternalPaymentStateFactory
    typealias PersistentValue = ExternalPayment

    let payment: ExternalPayment
    let coins: [Coin]
    let exactVoucherIndices: [CoinageKeyIndex]
    let isTerminal = false

    static func recyclingGroupId(for payment: ExternalPayment) -> CoinageTxGroupId {
        "\(payment.identifier):recycle"
    }

    func transit(
        with factory: ExternalPaymentStateFactory
    ) async -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        let groupId = Self.recyclingGroupId(for: payment)

        do {
            try await factory.recycler.recycleCoins(coins, groupId: groupId)

            guard try await !factory.durability.getOperationGroupStatuses(groupId).isEmpty else {
                return factory.makeFailedState(
                    payment: payment,
                    reason: coins.isEmpty ? "recycling group not found" : "recycling submission failed"
                )
            }

            for try await status in factory.recycler.observeRecycling(groupId: groupId) {
                switch status {
                case .pending:
                    continue
                case .incomplete:
                    return factory.makeFailedState(payment: payment, reason: "recycling incomplete")
                case let .allRecycled(recycled, finalized):
                    factory.logger?.debug(
                        "Payment \(payment.identifier): \(recycled.count) vouchers recycled, finalized \(finalized)"
                    )
                    return try await offboard(recycled: recycled, factory: factory)
                }
            }

            return factory.makeFailedState(payment: payment, reason: "recycling stream ended")
        } catch {
            return factory.makeFailedState(payment: payment, reason: error.localizedDescription)
        }
    }

    func memo() async -> ExternalPayment {
        var currentPayment = payment
        currentPayment.stage = .onboardCoins
        currentPayment.plannedVoucherIndices = exactVoucherIndices
        currentPayment.surplusInPlanks = 0
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}

private extension OnboardCoinsPaymentState {
    func offboard(
        recycled: [TrackedVoucher],
        factory: ExternalPaymentStateFactory
    ) async throws -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        let exact = try await factory.voucherService.fetchTracked(derivationIndices: Set(exactVoucherIndices))
        let available = exact + recycled

        do {
            let offboarding = try factory.planner.pickOffboarding(
                from: available,
                target: payment.amountInPlanks,
                context: factory.context
            )
            return factory.makeOffboardVouchersState(payment: payment, offboarding: offboarding)
        } catch ExternalPaymentPlannerError.insufficientVouchers {
            return factory.makeFailedState(payment: payment, reason: "insufficient balance after recycling")
        }
    }
}
