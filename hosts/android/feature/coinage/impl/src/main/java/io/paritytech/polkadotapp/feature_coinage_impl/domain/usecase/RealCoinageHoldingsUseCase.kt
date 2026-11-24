package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyCoins
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyVouchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldings
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingParamsProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingStrategyProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherReadinessUpdates
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherUsabilityContextFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.withReadinessUpdates
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/**
 * Keeps its own asset subscriptions for the same reason `RealTotalBalanceUseCase` does: a spend has to leave
 * the screen at once rather than on the evaluator's next tick. Only the gating verdicts come from there.
 */
class RealCoinageHoldingsUseCase @Inject constructor(
    private val coinageAssetsUseCase: CoinageAssetsUseCase,
    private val strategyProvider: RecyclingStrategyProvider,
    private val paramsProvider: RecyclingParamsProvider,
    private val evaluator: CoinRecyclingEvaluator,
    private val usabilityContextFactory: VoucherUsabilityContextFactory,
    private val readinessUpdates: VoucherReadinessUpdates,
) : CoinageHoldingsUseCase {
    override fun subscribeHoldings(): Flow<CoinageHoldings> = combine(
        coinageAssetsUseCase.subscribeCoins(),
        coinageAssetsUseCase.subscribeVouchers(),
        paramsProvider.paramsUpdates(),
        evaluator.verdicts,
        ::CoinageEvaluationInput,
    ).withReadinessUpdates(readinessUpdates, { it.vouchers }, { it.params })
        .map { (coins, vouchers, params, verdicts) ->
            val strategy = strategyProvider.voucherStrategyFor(params)

            // The list must not wait on a ring-capacity fetch before appearing.
            val usability = usabilityContextFactory.create(
                balanceEvaluationMode = BalanceEvaluationMode.IMMEDIATE,
                denominations = vouchers.mapToSet { it.voucher.recyclerValue },
            )

            CoinageHoldings(
                coins = coins.preClassifyCoins(),
                verdicts = verdicts,
                vouchers = vouchers.preClassifyVouchers(strategy, usability),
                canSpendWithConfirmation = strategy.allowsConfirmedSpend(),
            )
        }.distinctUntilChanged()
}
