package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoSessionRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealGetActiveSsoSessionsUseCase @Inject constructor(
    private val ssoSessionRepository: SsoSessionRepository
) : GetActiveSsoSessionsUseCase {
    override fun observeSessions(): Flow<List<ActiveSsoSession>> {
        return ssoSessionRepository.observeSessions().map { sessions ->
            sessions.map { it.toSession() }
        }
    }

    override fun observeSessionsChanged(): Flow<Unit> = ssoSessionRepository.observeSessionsChanged()

    override suspend fun getSessions(): List<ActiveSsoSession> {
        return ssoSessionRepository.getSessions().map { it.toSession() }
    }

    private fun SsoSessionData.toSession(): ActiveSsoSession {
        return ActiveSsoSession(
            id = id.value,
            statementAccountId = statementStorePublicKey.value.intoAccountId(),
            encryptionPublicKey = sharedSecretPublicKey,
            name = name,
            icon = icon,
            hostVersion = hostVersion,
            platformType = platformType,
            platformVersion = platformVersion,
            addedAt = addedAt,
            status = status,
            lastUpdate = lastUpdate,
        )
    }
}
