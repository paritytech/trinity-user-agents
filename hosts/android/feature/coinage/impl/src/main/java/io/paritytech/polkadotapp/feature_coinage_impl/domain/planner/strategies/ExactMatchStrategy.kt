package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import javax.inject.Inject

class ExactMatchStrategyFactory @Inject constructor(
    private val transactionService: CoinageTransactionService,
) {
    fun create(payload: StrategyType.ExactCoins): ExactMatchStrategy {
        return ExactMatchStrategy(payload, transactionService)
    }
}

class ExactMatchStrategy(
    payload: StrategyType.ExactCoins,
    private val transactionService: CoinageTransactionService,
) : TransferStrategy {
    private val coins = payload.coins

    /**
     * Schedules no extrinsic of ours: the coins are handed to the recipient as they are, so the only durable
     * record is the handoff mark. What becomes of them afterwards is read from the chain, not tracked here.
     *
     * Marks no region: a local handoff mark cannot stall.
     */
    context(_: StalenessReportCollector)
    override suspend fun schedule(params: TransferSubmissionParams): Result<ScheduledTransfer> {
        val handedOff = coins.map { OwnAsset.Coin(it.derivationIndex) }

        return transactionService.preCommitHandoff(handedOff)
            .map { commit -> ScheduledTransfer(coins.toMemoEntries(), commit, transactions = emptyList()) }
    }
}
