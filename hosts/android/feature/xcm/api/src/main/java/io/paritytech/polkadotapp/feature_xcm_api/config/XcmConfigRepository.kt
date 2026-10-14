package io.paritytech.polkadotapp.feature_xcm_api.config

import io.paritytech.polkadotapp.feature_xcm_api.config.model.GeneralXcmConfig
import kotlinx.coroutines.flow.Flow

interface XcmConfigRepository {
    suspend fun awaitXcmConfig(): GeneralXcmConfig

    fun xcmConfigFlow(): Flow<GeneralXcmConfig>
}
