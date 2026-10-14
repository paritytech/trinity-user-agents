package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.subscribeAccountBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.breakdownRoundDown
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.accountId
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.produceIn
import kotlinx.coroutines.flow.retryWhen
import kotlinx.coroutines.withTimeoutOrNull
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class, ExperimentalCoroutinesApi::class)
class RealOnboardingUseCase @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val voucherRepository: VoucherRepository,
    private val transactionService: CoinageTransactionService,
    private val coinAmountBreakdownUseCase: CoinAmountBreakdownUseCase,
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase,
    private val assetValueUseCase: CoinageAssetValueUseCase,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val submissionUseCase: CoinageOnboardingSubmissionUseCase,
    private val timeProvider: TimeProvider,
) : OnboardingUseCase {
    private companion object {
        /**
         * How long one pass waits for the account to hold everything still owed before onboarding whatever
         * it can cover.
         *
         * Reading the balance like a queue means a look is consumed once, so this bounds only how long one
         * pass holds out for a fully funded account — never how long onboarding goes on, which is the
         * caller's window.
         */
        val FUNDING_TIMEOUT = 30.seconds

        /** How long a dropped balance subscription waits before it is opened again. */
        val FUNDING_RESUBSCRIBE_DELAY = 1.seconds
    }

    override suspend fun onboard(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
    ): Result<Unit> {
        val chain = chainAssetProvider.chain()

        return targetDenominations(amount).flatMap { denominations ->
            submissionUseCase(denominations, signerSource, chain, CoinageOperationGroupId.generateNew())
        }
    }

    override fun onboardDurably(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> = channelFlow {
        send(CoinageTransferDetection.Detecting)

        val chain = chainAssetProvider.chain()

        val target = targetDenominations(amount)
            .logFailure("Failed to plan onboarding of $amount")
            .getOrElse {
                send(CoinageTransferDetection.NotClaimed)
                return@channelFlow
            }

        coinageLogI("Onboarding starting group=${groupId.value} vouchers=${target.size} until=$retryUntil")

        // Its a channel so we only process each balance update once
        // To prevent a case where failing submit would cause us to loop at CPU speed (since awaitFundedWithTimeout
        // would keep answering from the same look)
        val funding = subscribeTransferable(signerSource.accountId(chain)).produceIn(this)

        var settled: List<CoinageTransactionState>

        while (true) {
            settled = awaitKnownOperationsSettled(groupId, target, report = ::send)
            val outstanding = target.minusEach(settled.finalizedDenominations())

            // Every denomination has a voucher that finalized. Onboarding finished.
            if (outstanding.isEmpty()) break

            // Deliberately checked before attempting, where a claim would still try once. The funds are the
            // caller's own and nothing else is going to spend them, so a window that closed on them is the
            // end of it — attempting anyway would charge an account whose owner has already written the
            // operation off.
            if (timeProvider.now() >= retryUntil) {
                coinageLogW("Onboarding window closed group=${groupId.value} outstanding=${outstanding.size}")
                break
            }

            val affordable = awaitFundedWithTimeout(funding, outstanding)

            if (affordable.isNotEmpty()) {
                submit(affordable, signerSource, chain, groupId, isRetrying = settled.isNotEmpty())
            } else {
                coinageLogD("Onboarding still waiting for funds group=${groupId.value} outstanding=${outstanding.size}")
            }
        }

        // Nothing further will be attempted, so this is the last word — and the only place a shortfall may
        // be called final.
        val verdict = settled.toVerdict(target)
        logDetection(groupId, verdict)
        send(verdict)

        // The balance subscription never ends by itself, and this flow does not finish while a child of its
        // scope is still running — so without this a finished onboarding would hang instead of completing,
        // and the caller would never learn it is done.
        funding.cancel()
    }

    /** The denominations [amount] breaks into, one voucher each. Deterministic, so a retry plans the same set. */
    private suspend fun targetDenominations(amount: BigDecimal): Result<List<ValueExponent>> =
        coinAmountBreakdownUseCase.createCoinAmountBreakdown().map { it.breakdownRoundDown(amount) }

    /**
     * Reports the group until nothing in it can change, then hands back what it settled on.
     *
     * A group with no entries is already settled — that is a first attempt, and there is nothing to wait for.
     */
    private suspend fun awaitKnownOperationsSettled(
        groupId: CoinageOperationGroupId,
        target: List<ValueExponent>,
        report: suspend (CoinageTransferDetection) -> Unit,
    ): List<CoinageTransactionState> {
        return transactionService.subscribeOperationGroupStatuses(groupId)
            .onEach { states ->
                val detection = states.toProgress(target)
                logDetection(groupId, detection)
                report(detection)
            }
            .first { states -> states.none { it.status.isLive } }
    }

    /**
     * The denominations the account can pay for as of the next look that covers everything still owed, or as
     * of the best look taken within [FUNDING_TIMEOUT].
     *
     * Holding out for the whole remainder is deliberate: a transfer still landing is the ordinary reason an
     * account reads short, and onboarding half of it now would put the rest in a second batch for nothing.
     * Settling for the last look is equally deliberate — an operation that was underfunded from the start
     * should still move the money that is actually there.
     */
    private suspend fun awaitFundedWithTimeout(
        funding: ReceiveChannel<Balance>,
        outstanding: List<ValueExponent>,
    ): List<ValueExponent> {
        var affordable = emptyList<ValueExponent>()

        withTimeoutOrNull(FUNDING_TIMEOUT) {
            for (transferable in funding) {
                coinageLogD("Onboarding got transferable update: $transferable")

                // The newest look wins outright, even when it is smaller than the one before: the account
                // can be spent from elsewhere, and onboarding against the largest balance ever seen would
                // submit against money it no longer has.
                affordable = outstanding.affordableWith(transferable)

                if (affordable.size == outstanding.size) break
            }
        }

        return affordable
    }

    /** As much of the remainder as [transferable] covers, largest denomination first so the most value moves. */
    private suspend fun List<ValueExponent>.affordableWith(transferable: Balance): List<ValueExponent> {
        val converter = balanceConverterUseCase.create()
            .logFailure("Failed to price onboarding denominations")
            .getOrElse { return emptyList() }

        var remaining = transferable
        val affordable = mutableListOf<ValueExponent>()

        sortedDescending().forEach { denomination ->
            val price = converter.formatExponentToBalance(denomination)

            if (price <= remaining) {
                remaining -= price
                affordable += denomination
            }
        }

        return affordable
    }

    private suspend fun submit(
        denominations: List<ValueExponent>,
        signerSource: TransactionSignerSource.Signed,
        chain: Chain,
        groupId: CoinageOperationGroupId,
        isRetrying: Boolean,
    ) {
        coinageLogI("Onboarding submitting group=${groupId.value} vouchers=${denominations.size} retry=$isRetrying")

        submissionUseCase(denominations, signerSource, chain, groupId)
            .onFailure { coinageLogE("Onboarding submission failed group=${groupId.value}", it) }
    }

    /**
     * The external-asset balance the vouchers are minted out of.
     *
     * Never ends: the retry loop's only other exit is the caller's window, so a subscription that completed
     * would sit it there re-evaluating an empty channel rather than waiting for money to arrive.
     */
    private fun subscribeTransferable(accountId: AccountId): Flow<Balance> {
        return flow {
            coinageLogD("Subscribing to transferable balance of ${chainAssetProvider.chain().addressOf(accountId)}")

            val balanceType = tokenBalanceTypeRegistry.typeFor(chainAssetProvider.asset())

            emitAll(balanceType.subscribeAccountBalance(accountId).map { it.transferable })
        }.retryWhen { cause, _ ->
            coinageLogE("Onboarding lost sight of the funding balance", cause)
            delay(FUNDING_RESUBSCRIBE_DELAY)

            true
        }
    }

    /**
     * Denominations with a voucher of ours that finalized: the threshold for stopping.
     *
     * Deliberately stricter than [arrivedDenominations]. Report on inclusion, stop on finality — telling the
     * caller the money is onboarded a block after submission is the whole latency win, but giving up at that
     * point would leave the amount short for good if a fork took the block away.
     */
    private suspend fun List<CoinageTransactionState>.finalizedDenominations(): List<ValueExponent> =
        filter { it.status == DurableTxStatus.FINALIZED_SUCCESS }.mintedDenominations()

    /** Denominations with a voucher of ours in a block: the reporting threshold. */
    private suspend fun List<CoinageTransactionState>.arrivedDenominations(): List<ValueExponent> =
        filter { it.status.isArrived }.mintedDenominations()

    private suspend fun List<CoinageTransactionState>.mintedDenominations(): List<ValueExponent> {
        val indices = flatMap { it.outputs }
            .filterIsInstance<OwnAsset.Voucher>()
            .map { it.ringVrfIndex }

        return voucherRepository.getByRingVrfKeyIndices(indices).map { it.recyclerValue }
    }

    /**
     * What is true right now, reported on every ledger update.
     *
     * Nothing here may say onboarding is over — only the caller's loop knows that — so a shortfall is never
     * announced while another attempt could still make it up.
     */
    private suspend fun List<CoinageTransactionState>.toProgress(target: List<ValueExponent>): CoinageTransferDetection {
        val arrived = filter { it.status.isArrived }
        val outstanding = target.minusEach(arrivedDenominations())

        return when {
            outstanding.isEmpty() -> claimed(target, arrived)

            // Part of the amount is onboarded and the rest is waiting on a retry. Only worth saying when a
            // voucher actually failed: on the happy path a batch lands one voucher at a time, and reporting
            // the running total would walk the user through every inclusion of an onboarding that is simply
            // in progress.
            arrived.isNotEmpty() && any { it.status == DurableTxStatus.FAILURE } ->
                CoinageTransferDetection.ClaimingRest(valueMintedBy(arrived))

            isEmpty() -> CoinageTransferDetection.Detecting

            else -> CoinageTransferDetection.Claiming
        }
    }

    /**
     * The last word, once nothing further will be attempted.
     *
     * The same two questions as [toProgress], answered in the past tense — which is the only place a
     * shortfall may be called final.
     */
    private suspend fun List<CoinageTransactionState>.toVerdict(target: List<ValueExponent>): CoinageTransferDetection {
        val arrived = filter { it.status.isArrived }
        val notArrived = target.minusEach(arrivedDenominations())

        return when {
            notArrived.isEmpty() -> claimed(target, arrived)

            arrived.isNotEmpty() -> CoinageTransferDetection.ClaimedPartially(valueMintedBy(arrived))

            else -> CoinageTransferDetection.NotClaimed
        }
    }

    private suspend fun List<CoinageTransactionState>.claimed(
        target: List<ValueExponent>,
        arrived: List<CoinageTransactionState>,
    ) = CoinageTransferDetection.Claimed(
        amount = valueMintedBy(arrived),
        finalized = target.minusEach(finalizedDenominations()).isEmpty(),
    )

    private suspend fun valueMintedBy(states: List<CoinageTransactionState>) =
        assetValueUseCase.valueOf(states.flatMap { it.outputs })
            .logFailure("Failed to value onboarded vouchers")
            .getOrDefault(Balance.ZERO)

    private fun logDetection(groupId: CoinageOperationGroupId, detection: CoinageTransferDetection) {
        val group = groupId.value

        when (detection) {
            is CoinageTransferDetection.Detecting -> coinageLogD("Onboarding detecting group=$group")
            is CoinageTransferDetection.Claiming -> coinageLogD("Onboarding in flight group=$group")

            is CoinageTransferDetection.ClaimingRest ->
                coinageLogI("Onboarding partial group=$group onboarded=${detection.claimed}")

            is CoinageTransferDetection.Claimed ->
                coinageLogI("Onboarding complete group=$group amount=${detection.amount} finalized=${detection.finalized}")

            is CoinageTransferDetection.ClaimedPartially ->
                coinageLogW("Onboarding ended short group=$group onboarded=${detection.claimed}")

            is CoinageTransferDetection.NotClaimed -> coinageLogE("Onboarding failed group=$group")
        }
    }
}

/**
 * [other] removed occurrence by occurrence, so two vouchers of the same denomination stay two units of work.
 *
 * The plain set-like `minus` would drop both the moment one of them landed, and report an amount half
 * onboarded as fully onboarded.
 */
private fun List<ValueExponent>.minusEach(other: List<ValueExponent>): List<ValueExponent> {
    val removable = other.toMutableList()

    return filterNot { removable.remove(it) }
}
