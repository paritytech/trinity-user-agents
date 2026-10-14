package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinRecyclingStrategy
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot

/**
 * Stops discretionary recycling once the free unload allowance runs low.
 *
 * Quota is never shown to the user — it cannot be topped up or paid for, so naming it would only mislead —
 * which is why it is managed here instead. An empty verdict map is all this needs to say: the chain-limits
 * decorator wrapping it fills every unjudged coin with `ALLOW_USE` bar the ones it forces, which is exactly
 * "recycle only what must be recycled".
 *
 * Reading the allowance is the one chain hop on the verdict path that the balance does not already wait for,
 * so it is the read an [BalanceEvaluationMode.IMMEDIATE] pass exists to skip.
 */
class EnsureQuotaLimitsStrategy(
    private val inner: CoinRecyclingStrategy,
    private val quotaTracker: UnloadQuotaTracker,
) : CoinRecyclingStrategy by inner {
    context(conversion: CoinageBalanceConversionContext)
    override suspend fun evaluate(
        coins: List<Coin>,
        snapshot: RecyclingSnapshot,
        mode: BalanceEvaluationMode,
    ): RecyclingVerdicts {
        // An allowance we could not read is not evidence of an exhausted one. Keeping the user's choice on a
        // failed read costs at worst a rejected unload; overriding it would stop recycling on a hiccup.
        val quotaRunningLow = quotaTracker.isQuotaRunningLow()
            .logFailure("Can't read remaining unload quota")
            .getOrDefault(false)

        return if (quotaRunningLow) emptyMap() else inner.evaluate(coins, snapshot, mode)
    }
}
