package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import kotlinx.coroutines.flow.Flow

/**
 * The privacy strategy the user has chosen, app-wide.
 *
 * Changing it needs no sweep: the flow feeds the evaluator, so the whole live set is re-judged on the next
 * tick with no window in which part of the balance is judged by the old strategy and part by the new.
 */
interface CoinageRecyclingStrategySettings {
    fun strategyFlow(): Flow<RecyclingStrategyType>

    suspend fun getStrategy(): RecyclingStrategyType

    suspend fun setStrategy(type: RecyclingStrategyType): Result<Unit>
}
