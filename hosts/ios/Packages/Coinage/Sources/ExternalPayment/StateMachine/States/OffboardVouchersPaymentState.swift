import Foundation
import SDKLogger
import StateMachine
import SubstrateSdk

/// Unloads the planned vouchers to external asset at the destination.
///
/// Submits straight to ``OffboardVouchersForPaymentService``, which re-joins an already registered
/// group on its own (the relaunch path). Every outcome is a verdict: `.success` completes,
/// `.partialSuccess` persists what settled as `partiallyCompleted`, `.failed`, a submission failure
/// and any thrown error persist `failed`.
struct OffboardVouchersPaymentState: StateMachineState {
    typealias StateFactory = ExternalPaymentStateFactory
    typealias PersistentValue = ExternalPayment

    let payment: ExternalPayment
    let voucherIndices: [CoinageKeyIndex]
    /// Planner output: what the vouchers exceed the amount by. Persisted with the stage, never
    /// recomputed, so a relaunch submits exactly what was planned.
    let surplus: Balance
    let isTerminal = false

    func transit(
        with factory: ExternalPaymentStateFactory
    ) async -> AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment> {
        let service = OffboardVouchersForPaymentService(
            instanceId: factory.instanceId,
            voucherKeyFactory: factory.voucherKeyFactory,
            voucherService: factory.voucherService,
            voucherMinter: factory.voucherMinter,
            recyclerLoader: factory.recyclerLoader,
            txService: factory.durability,
            originFactory: factory.originFactory,
            quotaTracker: factory.quotaTracker,
            blockNumberProvider: factory.blockNumberProvider,
            denominationContext: factory.context,
            dateProvider: factory.dateProvider,
            logger: factory.logger
        )

        do {
            let vouchers = try await factory.voucherService
                .fetchTracked(derivationIndices: Set(voucherIndices))
                .map(\.voucher)

            switch try await service.execute(payment: payment, vouchers: vouchers, surplus: surplus) {
            case .success:
                var settled = payment
                settled.settledInPlanks = payment.amountInPlanks
                return factory.makeCompletedState(payment: settled)
            case let .partialSuccess(settledInPlanks, executed, total):
                var settled = payment
                settled.settledInPlanks = settledInPlanks
                return factory.makePartiallyCompletedState(
                    payment: settled,
                    reason: "\(executed) of \(total) unload transactions executed"
                )
            case .failed:
                return factory.makeFailedState(payment: payment, reason: "no unload transaction executed")
            }
        } catch {
            return factory.makeFailedState(payment: payment, reason: error.localizedDescription)
        }
    }

    func memo() async -> ExternalPayment {
        var currentPayment = payment
        currentPayment.stage = .offboardVouchers
        currentPayment.plannedVoucherIndices = voucherIndices
        currentPayment.surplusInPlanks = surplus
        currentPayment.updatedAt = Date()
        return currentPayment
    }
}
