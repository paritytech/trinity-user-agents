import ExtrinsicService
import Foundation
import FoundationExt
import KeyDerivation
import SDKLogger
import StateMachine
import SubstrateOperation
import SubstrateSdk

/// Factory providing all dependencies and methods to create external payment states.
///
/// Passed to each state's ``StateMachineState/transit(with:)`` so states
/// can create their successors without coupling to concrete types.
final class ExternalPaymentStateFactory {
    let instanceId: CoinageInstanceId
    let planner: ExternalPaymentPlanning
    let context: DenominationBreakdownContext
    let voucherService: VoucherServiceProtocol
    let recycler: CoinageRecyclingServicing
    let voucherKeyFactory: any VoucherKeyDeriving
    let voucherMinter: any VoucherMinting
    let recyclerLoader: RecyclerReadinessLoading
    let durability: any CoinageTxServicing
    let originFactory: OriginCreating
    let quotaTracker: any UnloadQuotaTracking
    let blockNumberProvider: BlockInfoProviding
    let dateProvider: any DateProviding
    let logger: SDKLoggerProtocol?

    init(
        instanceId: CoinageInstanceId,
        planner: ExternalPaymentPlanning,
        context: DenominationBreakdownContext,
        voucherService: VoucherServiceProtocol,
        recycler: CoinageRecyclingServicing,
        voucherKeyFactory: any VoucherKeyDeriving,
        voucherMinter: any VoucherMinting,
        recyclerLoader: RecyclerReadinessLoading,
        durability: any CoinageTxServicing,
        originFactory: OriginCreating,
        quotaTracker: any UnloadQuotaTracking,
        blockNumberProvider: BlockInfoProviding,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.instanceId = instanceId
        self.planner = planner
        self.context = context
        self.voucherService = voucherService
        self.recycler = recycler
        self.voucherKeyFactory = voucherKeyFactory
        self.voucherMinter = voucherMinter
        self.recyclerLoader = recyclerLoader
        self.durability = durability
        self.originFactory = originFactory
        self.quotaTracker = quotaTracker
        self.blockNumberProvider = blockNumberProvider
        self.dateProvider = dateProvider
        self.logger = logger
    }
}

// MARK: - State Creation

extension ExternalPaymentStateFactory {
    typealias ErasedState = AnyStateMachineState<ExternalPaymentStateFactory, ExternalPayment>

    func makePlanState(payment: ExternalPayment) -> ErasedState {
        AnyStateMachineState(PlanPaymentState(payment: payment))
    }

    func makeOnboardCoinsState(payment: ExternalPayment, coins: [Coin], exactVouchers: [Voucher]) -> ErasedState {
        AnyStateMachineState(OnboardCoinsPaymentState(
            payment: payment,
            coins: coins,
            exactVoucherIndices: exactVouchers.map(\.derivationIndex)
        ))
    }

    func makeOffboardVouchersState(payment: ExternalPayment, offboarding: VoucherOffboarding) -> ErasedState {
        AnyStateMachineState(OffboardVouchersPaymentState(
            payment: payment,
            voucherIndices: offboarding.vouchers.map(\.voucher.derivationIndex),
            surplus: offboarding.surplus
        ))
    }

    func makeCompletedState(payment: ExternalPayment) -> ErasedState {
        AnyStateMachineState(CompletedPaymentState(payment: payment))
    }

    func makeFailedState(payment: ExternalPayment, reason: String) -> ErasedState {
        logger?.error("Payment \(payment.identifier) failed: \(reason)")
        return AnyStateMachineState(FailedPaymentState(payment: payment, reason: reason))
    }

    func makePartiallyCompletedState(payment: ExternalPayment, reason: String) -> ErasedState {
        logger?
            .error(
                "Payment \(payment.identifier) short: settled \(payment.settledInPlanks) of \(payment.amountInPlanks) (\(reason))"
            )
        return AnyStateMachineState(PartiallyCompletedPaymentState(payment: payment, reason: reason))
    }

    /// Restores a state from a persisted ``ExternalPayment`` memo. Mid-flight stages carry their
    /// vouchers in the memo; the states re-join the durability groups they registered.
    func stateFromMemo(payment: ExternalPayment) -> ErasedState {
        switch payment.stage {
        case .plan:
            makePlanState(payment: payment)
        case .onboardCoins:
            AnyStateMachineState(OnboardCoinsPaymentState(
                payment: payment,
                coins: [],
                exactVoucherIndices: payment.plannedVoucherIndices
            ))
        case .offboardVouchers:
            AnyStateMachineState(OffboardVouchersPaymentState(
                payment: payment,
                voucherIndices: payment.plannedVoucherIndices,
                surplus: payment.surplusInPlanks
            ))
        case .completed:
            makeCompletedState(payment: payment)
        case .failed:
            makeFailedState(payment: payment, reason: payment.failureReason ?? "Unknown")
        case .partiallyCompleted:
            makePartiallyCompletedState(payment: payment, reason: payment.failureReason ?? "Partial")
        }
    }
}
