import BigInt
import DurableTransactionsTestSupport
import Foundation
import FoundationExt
import os
import SubstrateSdk
import Testing
@testable import Coinage

/// Real `ExternalPaymentService` and state machine over an in-memory store and the stubbed lowest
/// seams: scripted planner, scripted recycler, tracked coins/vouchers, and a group-aware durability
/// double.
struct ExternalPaymentHarness {
    let store: InMemoryExternalPaymentStore
    let txService: StubGroupTxService
    let recycler: StubCoinageRecyclingService
    let planner: StubExternalPaymentPlanner
    let coins: StubCoinService
    let vouchers: StubVoucherService
    let service: ExternalPaymentService
}

enum ExternalPaymentTestFactory {
    static let denomination = DenominationBreakdownContext(
        unit: 1,
        precision: 0,
        maxExponent: 6,
        minExponent: 0
    )

    static let destination = Data(repeating: 7, count: 32)
    static let freeState = CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)

    static func planks(_ exponent: Int16) -> Balance {
        denomination.valueInPlanks(for: exponent)
    }

    /// In a recycler with enough members and age to be usable under every preset.
    static func voucher(index: CoinageKeyIndex, exponent: Int16 = 3, inRecycler: Bool = true) -> Voucher {
        Voucher(
            exponent: exponent,
            derivationIndex: index,
            allocatedAt: Date(timeIntervalSince1970: 0),
            readyAt: .distantPast,
            remoteState: inRecycler
                ? .inRecycler(Voucher.Recycler(index: 1, membersCount: 64, enteredAt: Date(timeIntervalSince1970: 0)))
                : .unlocated,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
    }

    /// In a nearly empty ring it just entered: gaining privacy under `balanced` and `maxPrivacy`.
    static func gainingVoucher(index: CoinageKeyIndex, exponent: Int16 = 3) -> Voucher {
        voucher(index: index, exponent: exponent)
            .adjusting(state: .inRecycler(Voucher.Recycler(index: 1, membersCount: 1)))
    }

    static func tracked(_ voucher: Voucher, state: CoinageAssetState = freeState) -> TrackedVoucher {
        TrackedVoucher(voucher: voucher, state: state)
    }

    static func coin(index: CoinageKeyIndex, exponent: Int16 = 3, age: Int16? = 4, isOnchain: Bool = true) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: index,
            age: age,
            isOnchain: isOnchain,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
    }

    static func tracked(_ coin: Coin, state: CoinageAssetState = freeState) -> TrackedCoin {
        TrackedCoin(coin: coin, state: state)
    }

    static func payment(
        productId: String = "getcash.dot",
        paymentId: String = "0xaa",
        amount: Balance = planks(3),
        settled: Balance = 0,
        stage: ExternalPayment.Stage = .plan,
        plannedVoucherIndices: [CoinageKeyIndex] = [],
        surplus: Balance = 0,
        createdAt: Date = Date()
    ) -> ExternalPayment {
        ExternalPayment(
            productId: productId,
            paymentId: paymentId,
            amountInPlanks: amount,
            destination: destination,
            settledInPlanks: settled,
            stage: stage,
            plannedVoucherIndices: plannedVoucherIndices,
            surplusInPlanks: surplus,
            createdAt: createdAt
        )
    }

    static func unloadPreview(_ vouchers: [Voucher], surplus: Balance = 0) -> ExternalPaymentPreview {
        .unloadVouchers(VoucherOffboarding(vouchers: vouchers.map { tracked($0) }, surplus: surplus))
    }

    static func loadCoinsPreview(coins: [Coin], exactVouchers: [Voucher]) -> ExternalPaymentPreview {
        .loadCoins(coins: coins.map { tracked($0) }, exactVouchers: exactVouchers.map { tracked($0) })
    }

    static func recycleGroupId(for payment: ExternalPayment) -> CoinageTxGroupId {
        OnboardCoinsPaymentState.recyclingGroupId(for: payment)
    }

    static func unloadGroupId(for payment: ExternalPayment) -> CoinageTxGroupId {
        payment.identifier
    }

    static func makeStateFactory(
        planner: any ExternalPaymentPlanning,
        recycler: StubCoinageRecyclingService = StubCoinageRecyclingService(),
        txService: StubGroupTxService = StubGroupTxService(),
        vouchers: [Voucher] = []
    ) -> ExternalPaymentStateFactory {
        ExternalPaymentStateFactory(
            instanceId: 0,
            planner: planner,
            context: denomination,
            voucherService: StubVoucherService(vouchers: vouchers),
            recycler: recycler,
            voucherKeyFactory: StubVoucherKeyFactory(),
            voucherMinter: StubVoucherMinter(),
            recyclerLoader: StubRecyclerReadinessLoader(),
            durability: txService,
            originFactory: StubOriginFactory(),
            quotaTracker: StubUnloadQuotaTracker(),
            blockNumberProvider: StubBlockInfoProvider(),
            dateProvider: StubDateProvider(Date()),
            logger: nil
        )
    }

    static func makeHarness(
        store: InMemoryExternalPaymentStore = InMemoryExternalPaymentStore(),
        vouchers: [Voucher] = [],
        maxConsolidation: UInt32 = 100
    ) -> ExternalPaymentHarness {
        let txService = StubGroupTxService()
        let recycler = StubCoinageRecyclingService()
        let planner = StubExternalPaymentPlanner()
        let coins = StubCoinService()
        let voucherService = StubVoucherService(vouchers: vouchers)

        let machineFactory = ExternalPaymentStateMachineFactory(
            instanceId: 0,
            planner: planner,
            voucherService: voucherService,
            recycler: recycler,
            voucherKeyFactory: StubVoucherKeyFactory(),
            voucherMinter: StubVoucherMinter(),
            recyclerLoader: StubRecyclerReadinessLoader(maxConsolidationValue: maxConsolidation),
            extrinsicMonitor: StubExtrinsicMonitorFactory(),
            durability: txService,
            originFactory: StubOriginFactory(),
            quotaTracker: StubUnloadQuotaTracker(),
            blockNumberProvider: StubBlockInfoProvider(),
            dateProvider: StubDateProvider(Date()),
            logger: nil
        )

        let service = ExternalPaymentService(
            store: store,
            planner: planner,
            stateMachineFactory: machineFactory,
            logger: nil
        )

        return ExternalPaymentHarness(
            store: store,
            txService: txService,
            recycler: recycler,
            planner: planner,
            coins: coins,
            vouchers: voucherService,
            service: service
        )
    }
}
