package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.coinageBalanceOf
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyCoins
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyVouchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingStrategyProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherReadinessUpdates
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherUsabilityContextFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.withReadinessUpdates
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import org.jetbrains.annotations.VisibleForTesting
import javax.inject.Inject

/**
 * Keeps its own subscriptions rather than reading the evaluator's output: a spend has to leave the displayed
 * balance at once, not on the next evaluation. Only the gating verdicts are taken from the evaluator.
 */
class RealTotalBalanceUseCase @Inject constructor(
    private val coinageAssetsUseCase: CoinageAssetsUseCase,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
    private val strategyProvider: RecyclingStrategyProvider,
    private val settings: CoinageRecyclingStrategySettings,
    private val evaluator: CoinRecyclingEvaluator,
    private val usabilityContextFactory: VoucherUsabilityContextFactory,
    private val readinessUpdates: VoucherReadinessUpdates,
) : TotalBalanceUseCase {
    private data class Input(
        val coins: List<TrackedCoin>,
        val vouchers: List<TrackedVoucher>,
        val strategyType: RecyclingStrategyType,
        val verdicts: RecyclingVerdicts,
    )

    override fun subscribeTotalBalance(): Flow<Result<CoinageBalance>> = combine(
        coinageAssetsUseCase.subscribeCoins(),
        coinageAssetsUseCase.subscribeVouchers(),
        settings.strategyFlow(),
        evaluator.verdicts,
        ::Input,
    ).withReadinessUpdates(readinessUpdates, { it.vouchers }, { it.strategyType })
        .map { calculateCoinageBalance(it.coins, it.vouchers, it.strategyType, it.verdicts) }
        .distinctUntilChanged()

    override suspend fun getBalance(): Result<CoinageBalance> {
        return calculateCoinageBalance(
            coins = coinageAssetsUseCase.getCoins(),
            vouchers = coinageAssetsUseCase.getVouchers(),
            strategyType = settings.getStrategy(),
            verdicts = evaluator.verdicts.first(),
        )
    }

    /**
     * A coin the evaluator has not judged yet counts as processing, not available. The correction that
     * follows is upward; the other way round would show spendable balance and take it away a moment later.
     */
    @VisibleForTesting
    internal suspend fun calculateCoinageBalance(
        coins: List<TrackedCoin>,
        vouchers: List<TrackedVoucher>,
        strategyType: RecyclingStrategyType,
        verdicts: RecyclingVerdicts,
    ): Result<CoinageBalance> {
        if (coins.isEmpty() && vouchers.isEmpty()) return Result.success(CoinageBalance.EMPTY)

        return coinageBalanceConverterUseCase.create().map { conversionContext ->
            val strategy = strategyProvider.voucherStrategyFor(strategyType)
            val denominations = vouchers.mapToSet { it.voucher.recyclerValue }

            // We use IMMEDIATE since we don't want to delay balance computation
            val usability = usabilityContextFactory.create(BalanceEvaluationMode.IMMEDIATE, denominations)

            with(conversionContext) {
                coinageBalanceOf(
                    coins = coins.preClassifyCoins(),
                    verdicts = verdicts,
                    vouchers = vouchers.preClassifyVouchers(strategy, usability),
                    canSpendWithConfirmation = strategy.allowsConfirmedSpend(),
                )
            }
        }
    }
}
