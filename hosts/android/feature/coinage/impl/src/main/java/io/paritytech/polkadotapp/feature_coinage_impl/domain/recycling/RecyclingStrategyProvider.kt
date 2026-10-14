package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinRecyclingStrategy
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import javax.inject.Inject

/**
 * Builds the strategy for a chosen preset, wrapped in the two limits no preference may override.
 *
 * Assembly only: each limit reads what it needs when it is asked to decide, so nothing here has to know what
 * the chain will still accept or how much allowance is left.
 */
class RecyclingStrategyProvider @Inject constructor(
    private val forcedRecyclingAgeProvider: ForcedRecyclingAgeProvider,
    private val quotaTracker: UnloadQuotaTracker,
) {
    fun strategyFor(type: RecyclingStrategyType): CoinRecyclingStrategy = EnsureChainLimitsStrategy(
        inner = EnsureQuotaLimitsStrategy(
            inner = policyFor(type),
            quotaTracker = quotaTracker,
        ),
        forcedRecyclingAgeProvider = forcedRecyclingAgeProvider,
    )

    /**
     * The bare policy, for callers that only ask about voucher usability.
     *
     * Both decorators pass [CoinRecyclingStrategy.isVoucherUsable] through untouched, so answering that
     * question needs neither the chain limit nor a quota read — which is what keeps the balance off the
     * chain on its hot path.
     */
    fun voucherStrategyFor(type: RecyclingStrategyType): CoinRecyclingStrategy = policyFor(type)

    private fun policyFor(type: RecyclingStrategyType) =
        ParametricRecyclingStrategy(type.params, forcedRecyclingAgeProvider)
}
