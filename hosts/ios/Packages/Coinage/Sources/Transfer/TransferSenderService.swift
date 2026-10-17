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

    /// Native custody: registers the transfer's transactions under `groupId` and retains its recipient
    /// custody in one write, after `authorization` passes inside it. Replaying `custodyId` returns the
    /// retained memo without allocating or spending again.
    func execute(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?,
        custodyId: String,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws -> PreparedTransfer

    func retainedTransfer(
        custodyId: String,
        breakdownContext: DenominationBreakdownContext
    ) async throws -> PreparedTransfer?
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
            let prepared = try await prepare(result: result, breakdownContext: breakdownContext, native: nil)

            return PreparedTransfer(
                memo: prepared.memo,
                handoffCommit: prepared.strategy.handoffCommit,
                transactions: prepared.strategy.transactions,
                groupId: groupId,
                txService: txService
            )
        }
    }

    func execute(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?,
        custodyId: String,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws -> PreparedTransfer {
        try await markStallActivity("Execute transfer") {
            try Task.checkCancellation()
            guard !custodyId.isEmpty else { throw NativeTransferCustodyError.invalidRecord }
            if let retained = try await retainedTransfer(custodyId: custodyId, breakdownContext: breakdownContext) {
                try validateNativeAmount(retained.memo, result: result, context: breakdownContext)
                return retained
            }
            try Task.checkCancellation()

            let native = NativeTransferRequest(custodyId: custodyId, groupId: groupId, authorization: authorization)
            let prepared: (strategy: PreparedStrategy, memo: TransferMemo)
            do {
                prepared = try await prepare(result: result, breakdownContext: breakdownContext, native: native)
            } catch let TransferSenderServiceError.strategyFailed(error) {
                // Registration may have committed before the failure surfaced; a retained identity is
                // returned, never spent a second time.
                try Task.checkCancellation()
                if let retained = try await retainedTransfer(custodyId: custodyId, breakdownContext: breakdownContext) {
                    try validateNativeAmount(retained.memo, result: result, context: breakdownContext)
                    return retained
                }
                throw TransferSenderServiceError.strategyFailed(error)
            }
            try Task.checkCancellation()
            try validateNativeAmount(prepared.memo, result: result, context: breakdownContext)

            return PreparedTransfer(memo: prepared.memo, retaining: prepared.strategy.handoffCommit)
        }
    }

    func retainedTransfer(
        custodyId: String,
        breakdownContext: DenominationBreakdownContext
    ) async throws -> PreparedTransfer? {
        guard let retained = try await txService.retainedNativeTransfer(custodyId: custodyId) else { return nil }
        try Task.checkCancellation()
        let memo = try memoBuilder.buildMemo(from: retained.custody.memoEntries, breakdownContext: breakdownContext)
        return PreparedTransfer(memo: memo, retaining: retained.handoffCommit)
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

private extension TransferSenderService {
    /// Plans, mints and reserves the handoff, then builds the memo from what was minted. Native
    /// failures are not logged here; their caller decides whether a retained transfer answers them.
    func prepare(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        native: NativeTransferRequest?
    ) async throws -> (strategy: PreparedStrategy, memo: TransferMemo) {
        let plan: TransferPlan
        do {
            plan = try await planFactory.createPlan(for: result)
        } catch {
            if native == nil { logger?.error("Plan creation failed: \(error)") }
            throw TransferSenderServiceError.planCreationFailed(error)
        }

        // Mint outputs (persisted by the allocator) and reserve the handoff — everything that must land
        // before the memo (the keys) can leave. Normal transports keep a provisional handoff until their
        // carrying payload is durable; native custody is committed with its transactions here.
        let prepared: PreparedStrategy
        do {
            if native != nil { try Task.checkCancellation() }
            prepared = try await plan.strategy.prepare(native: native)
        } catch {
            if native == nil { logger?.error("Strategy preparation failed: \(error)") }
            throw TransferSenderServiceError.strategyFailed(error)
        }

        // Memo is built from what `prepare` just minted.
        let memo: TransferMemo
        do {
            memo = try memoBuilder.buildMemo(from: prepared.memoEntries, breakdownContext: breakdownContext)
        } catch {
            if native == nil { logger?.error("Memo building failed: \(error)") }
            throw TransferSenderServiceError.memoBuildingFailed(error)
        }

        return (prepared, memo)
    }

    func validateNativeAmount(
        _ memo: TransferMemo,
        result: CoinSelectionResult,
        context: DenominationBreakdownContext
    ) throws {
        let exponents: [Int16]
        switch result {
        case let .exactMatch(coins):
            exponents = coins.map(\.exponent)
        case let .split(wholeCoins, _, targetDenominations, _):
            exponents = wholeCoins.map(\.exponent) + targetDenominations.map(\.exponent)
        case let .unloadIntoCoins(coins, allocations):
            exponents = coins.map(\.exponent) + allocations.flatMap { $0.recipientDenominations.map(\.exponent) }
        }
        let expected = exponents.reduce(BigUInt.zero) { $0 + context.valueInPlanks(for: $1) }
        guard memo.totalValue == expected else { throw NativeTransferCustodyError.amountMismatch }
    }
}
