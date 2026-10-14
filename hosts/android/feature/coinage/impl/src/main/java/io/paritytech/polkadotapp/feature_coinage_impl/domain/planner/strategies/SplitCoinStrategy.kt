package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.plusHop
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.splitHop
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.consumeCoin
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.handOffCoins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.mintAndHandOffCoins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.CoinageSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class SplitCoinStrategyFactory @Inject constructor(
    private val transactionService: CoinageTransactionService,
    private val coinageTransactionFactory: CoinageTransaction.Factory,
) {
    fun create(payload: StrategyType.Split): SplitCoinStrategy = SplitCoinStrategy(
        transactionService = transactionService,
        payload = payload,
        coinageTransactionFactory = coinageTransactionFactory,
    )
}

class SplitCoinStrategy(
    private val transactionService: CoinageTransactionService,
    payload: StrategyType.Split,
    private val coinageTransactionFactory: CoinageTransaction.Factory,
) : TransferStrategy {
    private val coinToSplit = payload.splitFrom
    private val recipientDenominations = payload.recipientDenominations
    private val changeDenominations = payload.changeDenominations
    private val exactCoins = payload.exactCoins

    /**
     * Every output of the split inherits the split coin's origin and records this split as one more hop.
     *
     * The crowd is every coin the split produced, change included: on chain they leave together, so a coin
     * that stays with the user hides in the recipient's coins exactly as much as the other way round.
     */
    private val splitProvenance = coinToSplit.provenance.plusHop(
        splitHop(recipientDenominations.size + changeDenominations.size)
    )

    context(diagnostics: StalenessReportCollector)
    override suspend fun schedule(params: TransferSubmissionParams): Result<ScheduledTransfer> =
        diagnostics.markRegion(RCommon.string.coinage_stall_preparing_transfer) {
            val transaction = coinageTransactionFactory.newTransaction()

            val outputs = transaction.mintSplitOutputs().getOrElse { return Result.failure(it) }
            val assets = transaction.build()

            transactionService.preCommitHandoff(assets.handedOff).map { handoffCommit ->
                ScheduledTransfer(
                    entries = (exactCoins + outputs.recipient).toMemoEntries(),
                    handoffCommit = handoffCommit,
                    transactions = listOf(
                        CoinageScheduledTransactionRequest(
                            policy = CoinageSubmissionParams.splitPolicy(params),
                            inputs = assets.inputs,
                            outputs = assets.outputs,
                        )
                    ),
                )
            }
        }

    private suspend fun CoinageTransaction.mintSplitOutputs(): Result<TransferOutputs> = runCatching {
        consumeCoin(coinToSplit)
        handOffCoins(exactCoins)
        val recipientCoins = mintAndHandOffCoins(recipientDenominations, splitProvenance).getOrThrow()
        val changeCoins = mintCoins(changeDenominations, splitProvenance).getOrThrow()
        TransferOutputs(recipientCoins, changeCoins)
    }
}
