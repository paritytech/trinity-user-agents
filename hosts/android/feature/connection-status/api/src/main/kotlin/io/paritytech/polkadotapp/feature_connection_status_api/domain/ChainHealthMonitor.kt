package io.paritytech.polkadotapp.feature_connection_status_api.domain

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import kotlinx.coroutines.flow.Flow

interface ChainHealthMonitor {
    fun observeChainsHealth(): Flow<List<ChainHealth>>
}
