package io.paritytech.polkadotapp.feature_coinage_api.domain.debug

import kotlinx.coroutines.flow.Flow

interface CoinageDebugSettings {
    fun widgetsEnabledFlow(): Flow<Boolean>

    suspend fun areWidgetsEnabled(): Boolean

    suspend fun setWidgetsEnabled(enabled: Boolean)
}
