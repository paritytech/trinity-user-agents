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
        groupId: CoinageTxGroupId?,
        custodyId: String?,
        authorization: (@Sendable () throws -> Void)?
    ) async throws -> PreparedTransfer

    func retainedTransfer(
        custodyId: String,
        breakdownContext: DenominationBreakdownContext
    ) async throws -> PreparedTransfer?
}

extension TransferSenderServicing {
    func execute(
        result: CoinSelectionResult,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?,
        custodyId: String? = nil,
        authorization: (@Sendable () throws -> Void)? = nil
    ) async throws -> PreparedTransfer {
        try await execute(
            result: result,
            currentDate: .now,
            breakdownContext: breakdownContext,
            groupId: groupId,
            custodyId: custodyId,
            authorization: authorization
        )
    }
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
        groupId: CoinageTxGroupId?,
        custodyId: String? = nil,
        authorization: (@Sendable () throws -> Void)? = nil
    ) async throws -> PreparedTransfer {
        try await markStallActivity("Execute transfer") {
            if let custodyId {
                try Task.checkCancellation()
                guard !custodyId.isEmpty else { throw NativeTransferCustodyError.invalidRecord }
                if let retained = try await retainedTransfer(custodyId: custodyId, breakdownContext: breakdownContext) {
                    try validateNativeAmount(retained.memo, result: result, context: breakdownContext)
                    return retained
                }
                try Task.checkCancellation()
            }
            let plan: TransferPlan
            do {
                plan = try await planFactory.createPlan(for: result)
            } catch {
                if custodyId == nil { logger?.error("Plan creation failed: \(error)") }
                throw TransferSenderServiceError.planCreationFailed(error)
            }

            // Native custody is committed with registration; normal transports retain the existing
            // provisional handoff until their carrying payload is durable.
            let prepared: PreparedStrategy
            do {
                if custodyId != nil { try Task.checkCancellation() }
                prepared = try await plan.strategy.prepare(
                    groupId: groupId, custodyId: custodyId, authorization: authorization
                )
            } catch {
                if let custodyId {
                    try Task.checkCancellation()
                    if let retained = try await retainedTransfer(custodyId: custodyId, breakdownContext: breakdownContext) {
                        try validateNativeAmount(retained.memo, result: result, context: breakdownContext)
                        return retained
                    }
                }
                if custodyId == nil { logger?.error("Strategy preparation failed: \(error)") }
                throw TransferSenderServiceError.strategyFailed(error)
            }

            // Memo is built from what `prepare` just minted.
            let memo: TransferMemo
            do {
                memo = try memoBuilder.buildMemo(from: prepared.memoEntries, breakdownContext: breakdownContext)
            } catch {
                if custodyId == nil { logger?.error("Memo building failed: \(error)") }
                throw TransferSenderServiceError.memoBuildingFailed(error)
            }
            if custodyId != nil {
                try Task.checkCancellation()
                try validateNativeAmount(memo, result: result, context: breakdownContext)
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

    func retainedTransfer(
        custodyId: String,
        breakdownContext: DenominationBreakdownContext
    ) async throws -> PreparedTransfer? {
        guard let retained = try await txService.retainedNativeTransfer(custodyId: custodyId) else { return nil }
        try Task.checkCancellation()
        let memo = try memoBuilder.buildMemo(from: retained.custody.memoEntries, breakdownContext: breakdownContext)
        return PreparedTransfer(memo: memo, handoffCommit: retained.handoffCommit)
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
