package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoSessionRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import uniffi.truapi.PairedHostRecord
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.emitAll
import javax.inject.Inject

class RealGetActiveSsoSessionsUseCase @Inject constructor(
    private val runtimeSettings: ProductRuntimeSettings,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val ssoSessionRepository: SsoSessionRepository
) : GetActiveSsoSessionsUseCase {
    override fun observeSessions(): Flow<List<ActiveSsoSession>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return flow {
            runtimeProvider.runtime().getOrThrow()
            emitAll(runtimeProvider.pairedHosts.map { peers -> peers.map { it.toSession() } })
        }
        return ssoSessionRepository.observeSessions().map { sessions ->
            sessions.map { it.toSession() }
        }
    }

    override fun observeSessionsChanged(): Flow<Unit> = observeSessions().map { }

    override suspend fun getSessions(): List<ActiveSsoSession> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtimeProvider.runtime().getOrThrow().pairedHosts().map { it.toSession() }
        return ssoSessionRepository.getSessions().map { it.toSession() }
    }

    @OptIn(ExperimentalStdlibApi::class)
    private fun PairedHostRecord.toSession() = ActiveSsoSession(
        id = peerEncryption.toHexString(),
        statementAccountId = peerStatement.intoAccountId(),
        encryptionPublicKey = peerEncryption.requireX25519PublicKey(),
        name = metadata["hostName"].orEmpty(),
        icon = metadata["hostIcon"].orEmpty(),
        hostVersion = metadata["hostVersion"],
        platformType = metadata["platformType"],
        platformVersion = metadata["platformVersion"],
        addedAt = addedAt,
        status = DeviceStatus.ACTIVE,
        lastUpdate = updatedAt,
    )

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
