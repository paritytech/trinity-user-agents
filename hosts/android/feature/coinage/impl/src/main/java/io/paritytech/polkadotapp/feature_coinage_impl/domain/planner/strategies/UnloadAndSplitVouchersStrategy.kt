package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isInRecycler
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransactionAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.mintAndHandOffCoins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.CoinageSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class UnloadAndSplitVouchersStrategyFactory @Inject constructor(
    private val coinRepository: CoinRepository,
    private val transactionService: CoinageTransactionService,
    private val coinageTransactionFactory: CoinageTransaction.Factory,
    private val breakdownUseCase: CoinAmountBreakdownUseCase,
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase,
) {
    fun create(
        payload: StrategyType.UnloadAndSplit,
        chain: Chain,
    ) = UnloadAndSplitVouchersStrategy(
        payload = payload,
        chain = chain,
        coinRepository = coinRepository,
        transactionService = transactionService,
        coinageTransactionFactory = coinageTransactionFactory,
        breakdownUseCase = breakdownUseCase,
        balanceConverterUseCase = balanceConverterUseCase,
    )
}

class UnloadAndSplitVouchersStrategy(
    payload: StrategyType.UnloadAndSplit,
    private val chain: Chain,
    private val coinRepository: CoinRepository,
    private val transactionService: CoinageTransactionService,
    private val coinageTransactionFactory: CoinageTransaction.Factory,
    private val breakdownUseCase: CoinAmountBreakdownUseCase,
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase,
) : TransferStrategy {
    private val vouchers = payload.vouchersToUnload
    private val recipientAmount = payload.recipientAmount
    private val exactCoins = payload.exactCoins

    /**
     * One transaction per voucher batch, all built together by the unload policy once they are scheduled, so
     * they share one pinned block and person proof.
     */
    context(diagnostics: StalenessReportCollector)
    override suspend fun schedule(params: TransferSubmissionParams): Result<ScheduledTransfer> =
        diagnostics.markRegion(RCommon.string.coinage_stall_preparing_transfer) {
            runCatching { mintBatches() }.flatMap { minted ->
                transactionService.preCommitHandoff(minted.handedOff()).map { handoffCommit ->
                    val policy = CoinageSubmissionParams.unloadPolicy(params)

                    ScheduledTransfer(
                        entries = minted.memoEntries(),
                        handoffCommit = handoffCommit,
                        transactions = minted.map { batch ->
                            CoinageScheduledTransactionRequest(
                                policy = policy,
                                inputs = batch.assets.inputs,
                                outputs = batch.assets.outputs,
                            )
                        },
                    )
                }
            }
        }

    private suspend fun mintBatches(): List<MintedBatch> {
        require(vouchers.isNotEmpty()) { "TransferStrategyError.emptyVouchers" }
        require(vouchers.all { it.isInRecycler() }) { "TransferStrategyError.missingRecyclerInfo" }

        return resolveBatches().map { batch ->
            val transaction = coinageTransactionFactory.newTransaction()
            val outputs = transaction.mintGroupOutputs(batch)

            MintedBatch(batch, outputs, transaction.build())
        }
    }

    private fun List<MintedBatch>.handedOff(): List<OwnAsset> =
        exactCoins.map { OwnAsset.Coin(it.derivationIndex) } + flatMap { it.assets.handedOff }

    private fun List<MintedBatch>.memoEntries() = (exactCoins + flatMap { it.outputs.recipient }).toMemoEntries()

    private class MintedBatch(
        val batch: VoucherBatch,
        val outputs: TransferOutputs,
        val assets: CoinageTransactionAssets,
    )

    private suspend fun resolveBatches(): List<VoucherBatch> {
        val breakdown = breakdownUseCase.createCoinAmountBreakdown().getOrThrow()
        val conversionContext = balanceConverterUseCase.create().getOrThrow()
        val maxConsolidation = coinRepository.fetchMaxConsolidation(chain.id).getOrThrow()

        return VoucherBatchDistribution.distribute(
            vouchers = vouchers,
            recipientAmount = recipientAmount,
            maxConsolidation = maxConsolidation,
            breakdown = breakdown,
            conversionContext = conversionContext
        )
    }

    private suspend fun CoinageTransaction.mintGroupOutputs(batch: VoucherBatch): TransferOutputs {
        useVouchers(batch.vouchers)

        val provenance = batch.unloadProvenance()
        val recipientCoins = mintAndHandOffCoins(batch.recipientDenominations, provenance).getOrThrow()
        val changeCoins = mintCoins(batch.changeDenominations, provenance).getOrThrow()

        return TransferOutputs(recipientCoins, changeCoins)
    }

    /**
     * Coins leaving a recycler come out with that recycler's anonymity and no history yet.
     *
     * A batch is one recycler, and one tick writes the same fungibility to every voucher in a ring, so these
     * normally all agree. They can still disagree across ticks: a voucher that landed while the capacity or
     * unloaded-count read was failing keeps the default until a later tick fills it in. The lowest answers
     * for the batch rather than an arbitrary member, because understating privacy is the safe direction.
     *
     * Batches are deliberately not reconciled against each other: a coin from a fuller ring is genuinely
     * more private than one from an emptier ring in the same transfer.
     */
    private fun VoucherBatch.unloadProvenance(): CoinProvenance {
        val fungibility = vouchers.minOf { it.recyclerFungibility }

        return CoinProvenance.fromRecycler(fungibility)
    }
}
