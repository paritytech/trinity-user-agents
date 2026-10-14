import Foundation
import FoundationExt
import ExtrinsicService
import KeyDerivation
import SubstrateSdk
import SDKLogger
import SubstrateOperation

/// Factory interface for creating a `TransferPlan` (the strategy to execute) from a selection result.
protocol TransferPlanCreating {
    /// Builds the execution strategy for a coin selection result. Allocation, registration, and memo
    /// building all happen later, inside the strategy's `prepare`.
    func createPlan(for selectionResult: CoinSelectionResult) async throws -> TransferPlan
}

final class TransferPlanFactory {
    private let minter: any CoinMinting
    private let durability: any CoinageTxServicing
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        minter: any CoinMinting,
        durability: any CoinageTxServicing,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.minter = minter
        self.durability = durability
        self.dateProvider = dateProvider
        self.logger = logger
    }
}

// MARK: - TransferPlanCreating

extension TransferPlanFactory: TransferPlanCreating {
    func createPlan(for selectionResult: CoinSelectionResult) async throws -> TransferPlan {
        switch selectionResult {
        case let .exactMatch(coins):
            TransferPlan(strategy: ExactMatchStrategy(coins: coins, durability: durability))

        case let .split(wholeCoins, overflowCoin, targetDenominations, changeDenominations):
            TransferPlan(strategy: SplitCoinStrategy(
                wholeCoins: wholeCoins,
                overflowCoin: overflowCoin,
                targetDenominations: targetDenominations,
                changeDenominations: changeDenominations,
                minter: minter,
                txService: durability,
                dateProvider: dateProvider,
                logger: logger
            ))

        case let .unloadIntoCoins(coins, perGroupAllocations):
            TransferPlan(strategy: UnloadIntoCoinsStrategy(
                readyCoins: coins,
                perGroupAllocations: perGroupAllocations,
                minter: minter,
                txService: durability,
                dateProvider: dateProvider,
                logger: logger
            ))
        }
    }
}
