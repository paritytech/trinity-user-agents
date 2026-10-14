package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import javax.inject.Inject

/**
 * The age at which the chain stops accepting a coin, for the strategies that gate against it.
 *
 * The runtime publishes it as a view function, so it is read rather than known — and neither the wait nor
 * the failure may reach the balance, which renders off the verdict this feeds. [FALLBACK_RECYCLING_AGE] is
 * what the runtime has shipped for long enough that it gates the same coins in all but the moments after a
 * change, so standing on it costs at worst a rejected unload where a blank balance costs the screen.
 */
class ForcedRecyclingAgeProvider @Inject constructor(
    private val coinRepository: CoinRepository,
) {
    /** The published age, or [FALLBACK_RECYCLING_AGE] where it could not be read. */
    suspend fun get(): Int {
        return coinRepository.getCoinRecyclingAge()
            .logFailure(LOG_TAG)
            .getOrDefault(FALLBACK_RECYCLING_AGE)
    }

    /** What to gate on before the chain has answered. A [get] on the pass that follows corrects it. */
    fun immediateEstimate(): Int {
        return FALLBACK_RECYCLING_AGE
    }

    private companion object {
        const val LOG_TAG = "ForcedRecyclingAgeProvider"

        const val FALLBACK_RECYCLING_AGE = 14
    }
}

/** How long the caller can wait for the age decides which of the two it gets. */
suspend fun ForcedRecyclingAgeProvider.getForBalanceEvaluation(balanceEvaluationMode: BalanceEvaluationMode): Int {
    return when (balanceEvaluationMode) {
        BalanceEvaluationMode.COMPLETE -> get()
        BalanceEvaluationMode.IMMEDIATE -> immediateEstimate()
    }
}
