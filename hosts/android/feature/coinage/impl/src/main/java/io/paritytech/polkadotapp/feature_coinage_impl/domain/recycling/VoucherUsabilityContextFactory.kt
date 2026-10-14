package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherUsabilityContext
import javax.inject.Inject

class VoucherUsabilityContextFactory @Inject constructor(
    private val ringCapacityProvider: RingCapacityProvider,
    private val timeProvider: TimeProvider,
) {
    /**
     * Creates a [VoucherUsabilityContext] depending on [balanceEvaluationMode] for given [denominations]
     *
     * In case mode is [BalanceEvaluationMode.COMPLETE] awaits until the actual on-chain data for the context is known
     * In case mode is [BalanceEvaluationMode.IMMEDIATE] tries to use already known data, otherwise fallbacks to default values
     */
    suspend fun create(
        balanceEvaluationMode: BalanceEvaluationMode,
        denominations: Set<ValueExponent>
    ): VoucherUsabilityContext {
        return when (balanceEvaluationMode) {
            BalanceEvaluationMode.COMPLETE -> ringCapacityProvider.capacitiesFor(denominations)
                .map { FetchedVoucherUsabilityContext(it, timeProvider.now()) }
                .logFailure("Can't fetch ring capacities for recycler denominations")
                .getOrElse { ImmediateVoucherUsabilityContext(timeProvider.now()) }

            BalanceEvaluationMode.IMMEDIATE -> ringCapacityProvider.peekCapacitiesFor(denominations)
                ?.let { FetchedVoucherUsabilityContext(it, timeProvider.now()) } ?: ImmediateVoucherUsabilityContext(timeProvider.now())
        }
    }
}
