package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.hours
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

private const val COINAGE_LOG_TAG = "CoinageTransfer"

/**
 * How long a top-up keeps trying before whatever it has is all it will ever have.
 *
 * The same span for both sources, and measured from when the operation opened rather than from this
 * attempt — see [TopUpOperation].
 */
private val TOP_UP_RETRY_WINDOW = 1.hours

interface ExecuteTopUpUseCase {
    /**
     * Runs the top-up and reports it, ending when nothing further will be attempted.
     *
     * Completion is what says the top-up is over; the last status is its verdict. Calling this twice for the
     * same operation rejoins the transactions the first run registered rather than paying twice, which is
     * what makes picking an interrupted top-up back up safe.
     */
    fun execute(operation: TopUpOperation, source: TopUpSource): Flow<TopUpStatus>

    /**
     * Where a top-up has got to, read off the ledger alone.
     *
     * For a top-up that cannot be run — its source is gone — where what its transactions came to is still
     * the honest answer. What the group minted is what the user got, and what was asked for is recorded, so
     * this needs nothing that was thrown away.
     */
    suspend fun statusOf(operation: TopUpOperation): TopUpStatus
}

@OptIn(ExperimentalTime::class)
class RealExecuteTopUpUseCase @Inject constructor(
    private val claimReceivedCoinsUseCase: ClaimReceivedCoinsUseCase,
    private val onboardingUseCase: OnboardingUseCase,
    private val transactionService: CoinageTransactionService,
    private val assetValueUseCase: CoinageAssetValueUseCase,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : ExecuteTopUpUseCase {
    override fun execute(operation: TopUpOperation, source: TopUpSource): Flow<TopUpStatus> {
        val amount = operation.amount
        val groupId = operation.groupId
        val retryUntil = operation.startedAt + TOP_UP_RETRY_WINDOW

        Timber.tag(COINAGE_LOG_TAG)
            .i("Top-up running group=${groupId.value} amount=$amount until=$retryUntil")

        val detections = when (source) {
            is TopUpSource.Onboard -> onboard(source.signerSource, amount, groupId, retryUntil)

            // Claiming already ends on its own window, so it needs no bound of its own here. Nothing is
            // blocked on this call any more — the product follows the status instead of waiting on a reply.
            is TopUpSource.Coins -> claimReceivedCoinsUseCase.claim(source.coinKeys, groupId, retryUntil)
        }

        return detections
            .map { it.toStatus(amount) }
            .distinctUntilChanged()
            .onEach { Timber.tag(COINAGE_LOG_TAG).i("Top-up status group=${groupId.value} status=$it") }
    }

    override suspend fun statusOf(operation: TopUpOperation): TopUpStatus {
        val groupId = operation.groupId

        val entries = transactionService.getOperationGroupStatuses(groupId)
            .logFailure("Failed to read the top-up group ${groupId.value}")
            .getOrEmpty()

        val arrived = entries.filter { it.status.isArrived }
        val credited = assetValueUseCase.valueOf(arrived.flatMap { it.outputs })
            .logFailure("Failed to value what the top-up minted")
            .getOrDefault(Balance.ZERO)

        return when {
            credited >= operation.amount ->
                TopUpStatus.Claimed(finalized = entries.all { it.status == DurableTxStatus.FINALIZED_SUCCESS })

            credited.isPositive() -> TopUpStatus.ClaimedPartially(credited)

            else -> TopUpStatus.NotClaimed
        }
    }

    private fun onboard(
        signerSource: TransactionSignerSource.Signed,
        amount: Balance,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> = flow {
        val decimalAmount = chainAssetProvider.asset().amountFromPlanks(amount)

        emitAll(onboardingUseCase.onboardDurably(decimalAmount, signerSource, groupId, retryUntil))
    }
}

/**
 * How much of what was asked for is the user's, in the vocabulary RFC-0006 gives products.
 *
 * The detection's own amount is what actually landed, which can fall short of what the product asked for —
 * an underfunded account, coins worth less than the sender claimed. Only a verdict may call that partial:
 * while claiming is still going the rest can still arrive, so a shortfall reports as progress.
 */
private fun CoinageTransferDetection.toStatus(expected: Balance): TopUpStatus = when (this) {
    is CoinageTransferDetection.Detecting -> TopUpStatus.Detecting

    is CoinageTransferDetection.Claiming -> TopUpStatus.Claiming

    // Part landed and the rest is retrying. Products are given no variant for that, and it is a claim in
    // progress either way — the amount it carries is not the last word.
    is CoinageTransferDetection.ClaimingRest -> TopUpStatus.Claiming

    is CoinageTransferDetection.Claimed -> when {
        amount >= expected -> TopUpStatus.Claimed(finalized)
        finalized -> TopUpStatus.ClaimedPartially(amount)
        else -> TopUpStatus.Claiming
    }

    is CoinageTransferDetection.ClaimedPartially -> TopUpStatus.ClaimedPartially(claimed)

    is CoinageTransferDetection.NotClaimed -> TopUpStatus.NotClaimed
}
