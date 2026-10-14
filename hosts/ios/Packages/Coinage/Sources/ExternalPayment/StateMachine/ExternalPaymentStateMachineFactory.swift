import ExtrinsicService
import Foundation
import FoundationExt
import KeyDerivation
import SDKLogger
import StateMachine
import SubstrateOperation

/// Creates configured ``StateMachine`` instances for external payments.
///
/// Constructs the ``ExternalPaymentStateFactory`` with all dependencies,
/// loads the initial state from the store, and returns a ready-to-run machine.
final class ExternalPaymentStateMachineFactory: ExternalPaymentStateMachineCreating {
    private let instanceId: CoinageInstanceId
    private let planner: ExternalPaymentPlanning
    private let voucherService: VoucherServiceProtocol
    private let recycler: CoinageRecyclingServicing
    private let voucherKeyFactory: any VoucherKeyDeriving
    private let voucherMinter: any VoucherMinting
    private let recyclerLoader: RecyclerReadinessLoading
    private let extrinsicMonitor: ExtrinsicSubmitMonitorFactoryProtocol
    private let durability: any CoinageTxServicing
    private let originFactory: OriginCreating
    private let quotaTracker: any UnloadQuotaTracking
    private let blockNumberProvider: BlockInfoProviding
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        instanceId: CoinageInstanceId,
        planner: ExternalPaymentPlanning,
        voucherService: VoucherServiceProtocol,
        recycler: CoinageRecyclingServicing,
        voucherKeyFactory: any VoucherKeyDeriving,
        voucherMinter: any VoucherMinting,
        recyclerLoader: RecyclerReadinessLoading,
        extrinsicMonitor: ExtrinsicSubmitMonitorFactoryProtocol,
        durability: any CoinageTxServicing,
        originFactory: OriginCreating,
        quotaTracker: any UnloadQuotaTracking,
        blockNumberProvider: BlockInfoProviding,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.instanceId = instanceId
        self.planner = planner
        self.voucherService = voucherService
        self.recycler = recycler
        self.voucherKeyFactory = voucherKeyFactory
        self.voucherMinter = voucherMinter
        self.recyclerLoader = recyclerLoader
        self.extrinsicMonitor = extrinsicMonitor
        self.durability = durability
        self.originFactory = originFactory
        self.quotaTracker = quotaTracker
        self.blockNumberProvider = blockNumberProvider
        self.dateProvider = dateProvider
        self.logger = logger
    }

    func createStateMachine(
        for paymentId: String,
        store: ExternalPaymentStoring,
        context: DenominationBreakdownContext
    ) async throws -> StateMachine<ExternalPaymentStateFactory, ExternalPayment> {
        let stateStore = ExternalPaymentStateStore(paymentId: paymentId, store: store)
        let payment = try await stateStore.loadStateMemo()
        let stateFactory = makeStateFactory(context: context)
        let initialState = stateFactory.stateFromMemo(payment: payment)

        return StateMachine(
            initialState: initialState,
            factory: stateFactory,
            store: AnyStateMachineStoring(stateStore)
        )
    }
}

private extension ExternalPaymentStateMachineFactory {
    func makeStateFactory(context: DenominationBreakdownContext) -> ExternalPaymentStateFactory {
        ExternalPaymentStateFactory(
            instanceId: instanceId,
            planner: planner,
            context: context,
            voucherService: voucherService,
            recycler: recycler,
            voucherKeyFactory: voucherKeyFactory,
            voucherMinter: voucherMinter,
            recyclerLoader: recyclerLoader,
            durability: durability,
            originFactory: originFactory,
            quotaTracker: quotaTracker,
            blockNumberProvider: blockNumberProvider,
            dateProvider: dateProvider,
            logger: logger
        )
    }
}
