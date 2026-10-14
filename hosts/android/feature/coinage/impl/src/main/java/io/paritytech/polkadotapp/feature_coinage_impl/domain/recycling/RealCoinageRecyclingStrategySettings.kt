package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.RecyclingStrategyStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import javax.inject.Inject

class RealCoinageRecyclingStrategySettings @Inject constructor(
    private val storage: RecyclingStrategyStorage,
) : CoinageRecyclingStrategySettings {
    override fun strategyFlow(): Flow<RecyclingStrategyType> = storage.valueFlow().filterNotNull()

    override suspend fun getStrategy(): RecyclingStrategyType = storage.requireValue()

    override suspend fun setStrategy(type: RecyclingStrategyType): Result<Unit> = runCancellableCatching {
        storage.saveValue(type)
    }
}
