import Foundation
import BigInt
import SubstrateSdk
import SDKLogger
import StructuredConcurrency

/// Protocol for a coin unload to complete transfer.
protocol TransferSenderServicing: Actor {
    /// Preview the coin selection strategy without executing.
    ///
    /// - Parameters:
    ///   - amount: Amount to preview
    ///   - availableCoins: Coins available for selection
    ///   - availableVouchers: Vouchers available for selection
    ///   - breakdownContext: Context for denomination breakdown
    /// - Returns: The coin selection result
    /// - Throws: CoinSelectionError on failure
    func previewStrategy(
        amount: BigUInt,
        availableCoins: [TrackedCoin],
        availableVouchers: [TrackedVoucher],
        breakdownContext: DenominationBreakdownContext
    ) async throws -> CoinSelectionResult

    /// Execute a transfer from a pre-computed coin selection result, skipping coin selection.
    /// Returns the memo plus the provisional handoff to commit once the memo is durable.
    /// `groupId` labels the transaction(s) this transfer registers (the message id), or `nil`.
    func execute(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId
    ) async throws -> PreparedTransfer
}

/// Orchestrates the complete coin transfer sender flow.
///
/// Flow:
/// 1. Select coins via CoinSelector → CoinSelectionResult
/// 2. Create plan via TransferPlanFactory → TransferPlan (strategy + memo entries)
/// 3. Execute strategy (persists state via context)
/// 4. Build memo from planned entries via MemoBuilder
/// 5. Return memo for recipient
actor TransferSenderService {
    private let coinSelector: CoinSelecting
    private let planFactory: TransferPlanCreating
    private let memoBuilder: MemoBuilding
    private let recyclerLoader: RecyclerReadinessLoading
    private let txService: any CoinageTxServicing
    private let logger: SDKLoggerProtocol?

    private var cachedLimits: UnloadCallLimits?

    init(
        coinSelector: CoinSelecting,
        planFactory: TransferPlanCreating,
        memoBuilder: MemoBuilding,
        recyclerLoader: RecyclerReadinessLoading,
        txService: any CoinageTxServicing,
        logger: SDKLoggerProtocol?
    ) {
        self.coinSelector = coinSelector
        self.planFactory = planFactory
        self.memoBuilder = memoBuilder
        self.recyclerLoader = recyclerLoader
        self.txService = txService
        self.logger = logger
    }
}

private extension TransferSenderService {
    /// The pallet bounds one unload call must respect, read once per service.
    func unloadCallLimits() async throws -> UnloadCallLimits {
        if let cached = cachedLimits {
            return cached
        }
        let limits = try await UnloadCallLimits(
            maxVouchersPerCall: max(Int(recyclerLoader.maxConsolidation()), 1),
            maxOutputsPerCall: max(Int(recyclerLoader.maxSplitOutputs()), 1)
        )
        cachedLimits = limits
        return limits
    }
}

extension TransferSenderService: TransferSenderServicing {
    func execute(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId
    ) async throws -> PreparedTransfer {
        try await markStallActivity("Execute transfer") {
            let plan: TransferPlan
            do {
                plan = try await planFactory.createPlan(for: result)
            } catch {
                logger?.error("Plan creation failed: \(error)")
                throw TransferSenderServiceError.planCreationFailed(error)
            }

            // Mint outputs (persisted by the allocator), fire the background-tracked submission, and
            // pre-commit the handoff — everything that must land before the memo (the keys) can leave.
            // A failure leaves registered entries and a provisional handoff, both resolved by the
            // recovery pass / relaunch.
            let prepared: PreparedStrategy
            do {
                prepared = try await plan.strategy.prepare()
            } catch {
                logger?.error("Strategy preparation failed: \(error)")
                throw TransferSenderServiceError.strategyFailed(error)
            }

            // Memo is built from what `prepare` just minted.
            let memo: TransferMemo
            do {
                memo = try memoBuilder.buildMemo(from: prepared.memoEntries, breakdownContext: breakdownContext)
            } catch {
                logger?.error("Memo building failed: \(error)")
                throw TransferSenderServiceError.memoBuildingFailed(error)
            }

            return PreparedTransfer(
                memo: memo,
                handoffCommit: prepared.handoffCommit,
                transactions: prepared.transactions,
                groupId: groupId,
                txService: txService
            )
        }
    }

    func previewStrategy(
        amount: BigUInt,
        availableCoins: [TrackedCoin],
        availableVouchers: [TrackedVoucher],
        breakdownContext: DenominationBreakdownContext
    ) async throws -> CoinSelectionResult {
        let input = try await SelectCoinsInput(
            amount: amount,
            coins: availableCoins,
            vouchers: availableVouchers,
            breakdownContext: breakdownContext,
            limits: unloadCallLimits()
        )

        return try await coinSelector.selectCoins(input)
    }
}
