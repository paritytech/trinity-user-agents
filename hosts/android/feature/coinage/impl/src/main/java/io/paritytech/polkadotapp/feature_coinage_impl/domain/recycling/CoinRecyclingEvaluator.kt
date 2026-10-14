package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.throttleLatest
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.totalBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyCoins
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyVouchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds

private val EVALUATION_INTERVAL = 5.seconds

private const val LOG_TAG = "CoinRecyclingEvaluator"

private val RECYCLING_VERDICTS = setOf(CoinRecyclingState.MUST_RECYCLE, CoinRecyclingState.TO_RECYCLE)

/**
 * Decides which coins are held back for the recycler, and starts recycling the ones that are.
 *
 * The verdict is recomputed rather than stored: it depends on how much of the balance is already tied up
 * and on the ages of every other live coin, both of which move, so a decision written once would be wrong
 * as soon as either did.
 */
@Singleton
class CoinRecyclingEvaluator @Inject constructor(
    private val coinageAssetsUseCase: CoinageAssetsUseCase,
    private val settings: CoinageRecyclingStrategySettings,
    private val strategyProvider: RecyclingStrategyProvider,
    private val usabilityContextFactory: VoucherUsabilityContextFactory,
    private val balanceConverter: CoinageBalanceConverterUseCase,
    private val recyclingUseCase: CoinageRecyclingUseCase,
    private val dispatchers: CoroutineDispatchers,
    private val readinessUpdates: VoucherReadinessUpdates,
) {
    private class Assets(
        val coins: List<TrackedCoin>,
        val vouchers: List<TrackedVoucher>,
    ) {
        val isEmpty: Boolean
            get() = coins.isEmpty() && vouchers.isEmpty()
    }

    private class Input(
        val assets: Assets,
        val strategyType: RecyclingStrategyType,
    )

    /**
     * Null until the first evaluation lands. Consumers must wait rather than read it as "nothing is
     * available" — a balance that shows zero and then corrects itself is worse than one that waits.
     */
    private val verdictsState = MutableStateFlow<RecyclingVerdicts?>(null)

    val verdicts: Flow<RecyclingVerdicts> = verdictsState.filterNotNull()

    context(scope: ComputationalScope)
    fun start() {
        scope.launch(dispatchers.computation) {
            combine(coinageAssetsUseCase.subscribeCoins(), coinageAssetsUseCase.subscribeVouchers(), ::Assets)
                .throttleLatest(EVALUATION_INTERVAL)
                // Outside the throttle: switching strategy should re-judge the wallet at once rather than up
                // to an interval later. Only the asset churn needs damping.
                .combine(settings.strategyFlow(), ::Input)
                .withReadinessUpdates(readinessUpdates, { it.assets.vouchers }, { it.strategyType })
                .collect { input ->
                    // Both passes run off the same input rather than waiting for the next emission: the
                    // wallet may not change again for minutes, and the immediate verdicts must not stand
                    // that long.
                    if (verdictsState.value == null) publishImmediate(input)

                    evaluate(input, BalanceEvaluationMode.COMPLETE)
                        .onSuccess { verdicts ->
                            verdictsState.value = verdicts

                            // Deliberately not a collector of verdictsState: a StateFlow drops equal values,
                            // so a retry after a failed recycle would never reach one.
                            recycleGated(input, verdicts)
                        }
                        .logFailure(LOG_TAG)
                }
        }
    }

    /**
     * Gets a balance on screen without waiting for the chain, at the cost of a verdict that only the chain's
     * own age limit has shaped — so a coin the policy would hold back shows as spendable until the complete
     * pass moves it. Nothing is recycled off it: recycling is a chain write of its own, so starting it here
     * would buy nothing over the pass that follows immediately.
     */
    private suspend fun publishImmediate(input: Input) {
        evaluate(input, BalanceEvaluationMode.IMMEDIATE)
            .onSuccess { verdictsState.value = it }
            .logFailure(LOG_TAG)
    }

    private suspend fun evaluate(input: Input, mode: BalanceEvaluationMode): Result<RecyclingVerdicts> {
        if (input.assets.isEmpty) {
            coinageLogD("Recycling evaluation empty-assets")
            return Result.success(emptyMap())
        }

        val strategy = strategyProvider.strategyFor(input.strategyType)
        val conversion = balanceConverter.create().getOrElse { return Result.failure(it) }

        val denominations = input.assets.vouchers.mapToSet { it.voucher.recyclerValue }
        val usability = usabilityContextFactory.create(mode, denominations)

        val coinBuckets = input.assets.coins.preClassifyCoins()
        val voucherBuckets = input.assets.vouchers.preClassifyVouchers(strategy, usability)

        return with(conversion) {
            // Measured before this pass gates anything; evaluate accumulates into it, so the pending
            // balance it leaves behind is the same number the ceiling was checked against.
            val snapshot = RecyclingSnapshot(
                total = coinBuckets.total.totalBalance() + voucherBuckets.total.totalBalance(),
                unavailable = coinBuckets.minting.totalBalance() +
                    voucherBuckets.minting.totalBalance() +
                    voucherBuckets.gainingPrivacy.totalBalance(),
            )

            val verdicts = strategy.evaluate(coinBuckets.minted, snapshot, mode)

            coinageLogD(
                "Recycling evaluation mode=$mode strategy=${input.strategyType}" +
                    " minted=${coinBuckets.minted.size} arriving=${coinBuckets.minting.size}" +
                    " vouchers(usable=${voucherBuckets.usable.size} gaining=${voucherBuckets.gainingPrivacy.size})" +
                    " verdicts=${verdicts.values.groupingBy { it }.eachCount()}"
            )

            Result.success(verdicts)
        }
    }

    /**
     * Gating a coin takes it out of the spendable balance immediately, so recycling has to follow at once —
     * otherwise the user watches money move to "gaining privacy" and sit there. A failed attempt needs no
     * retry of its own: the next evaluation tries again.
     */
    private suspend fun recycleGated(input: Input, verdicts: RecyclingVerdicts) {
        // Both recycling verdicts, not just the discretionary one: a coin past the chain's age limit is the
        // one that most needs to go, and it is the reason the user cannot spend it.
        val gated = verdicts.filterValues { it in RECYCLING_VERDICTS }.keys
        if (gated.isEmpty()) return

        val coins = input.assets.coins.map { it.coin }.filter { it.derivationIndex in gated }

        coinageLogD("Recycling ${coins.size} gated coin(s)")

        // No quota is spent here: recycling loads a coin into a recycler, it does not unload anything.
        recyclingUseCase.recycle(coins).logFailure(LOG_TAG)
    }
}
