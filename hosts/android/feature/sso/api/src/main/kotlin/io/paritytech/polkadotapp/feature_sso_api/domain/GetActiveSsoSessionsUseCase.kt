package io.paritytech.polkadotapp.feature_sso_api.domain

import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import kotlinx.coroutines.flow.Flow

interface GetActiveSsoSessionsUseCase {
    fun observeSessions(): Flow<List<ActiveSsoSession>>

    fun observeSessionsChanged(): Flow<Unit>

    suspend fun getSessions(): List<ActiveSsoSession>
}
