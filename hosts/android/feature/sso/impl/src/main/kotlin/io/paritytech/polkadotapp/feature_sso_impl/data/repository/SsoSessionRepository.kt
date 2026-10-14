package io.paritytech.polkadotapp.feature_sso_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.utils.mapToUnit
import io.paritytech.polkadotapp.database.dao.SsoSessionDao
import io.paritytech.polkadotapp.database.model.SsoSessionLocal
import io.paritytech.polkadotapp.database.model.SsoSessionMetadataLocal
import io.paritytech.polkadotapp.database.model.SsoSessionWithMetadata
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeMetadata
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class SsoSessionRepository @Inject constructor(
    private val ssoSessionDao: SsoSessionDao,
) {
    fun observeSessions(): Flow<List<SsoSessionData>> {
        return ssoSessionDao.observeAll().map { sessions -> sessions.map { it.toDomain() } }
    }

    fun observeSessionsChanged(): Flow<Unit> = ssoSessionDao.observeSessionCount().distinctUntilChanged().mapToUnit()

    suspend fun getSessions(): List<SsoSessionData> {
        return ssoSessionDao.getAll().map { it.toDomain() }
    }

    suspend fun getSessionByStatementAccountId(statementAccountId: AccountId): SsoSessionData? {
        return ssoSessionDao.getByStatementStorePublicKey(statementAccountId.value)?.toDomain()
    }

    suspend fun saveSession(session: SsoSessionData) {
        ssoSessionDao.upsert(session.toLocal(), session.metadataToLocal())
    }

    suspend fun deleteSession(sharedSecretPublicKey: EncodedPublicKey) {
        ssoSessionDao.delete(sharedSecretPublicKey.value)
    }

    suspend fun deleteSession(sessionId: SsoSessionId) {
        ssoSessionDao.delete(sessionId.toRawSharedSecretPublicKey())
    }

    suspend fun deleteSessionByStatementAccountId(statementAccountId: AccountId) {
        ssoSessionDao.deleteByStatementStorePublicKey(statementAccountId.value)
    }

    private fun SsoSessionWithMetadata.toDomain(): SsoSessionData {
        return SsoSessionData(
            sharedSecretPublicKey = session.sharedSecretPublicKey.requireX25519PublicKey(),
            statementStorePublicKey = EncodedPublicKey(session.statementStorePublicKey),
            metadata = metadata.toDomain(),
            addedAt = session.addedAt,
            status = DeviceStatus.valueOf(session.status),
            lastUpdate = session.lastUpdate,
            outgoingUpdateTime = session.outgoingUpdateTime,
            lastSyncOfferId = session.lastSyncOfferId,
        )
    }

    private fun List<SsoSessionMetadataLocal>.toDomain(): HandshakeMetadata {
        return HandshakeMetadata(
            entries = associate { entry -> entry.key.toMetadataKey() to entry.value }
        )
    }

    private fun SsoSessionData.toLocal(): SsoSessionLocal {
        return SsoSessionLocal(
            sharedSecretPublicKey = sharedSecretPublicKey.bytes.value,
            statementStorePublicKey = statementStorePublicKey.value,
            addedAt = addedAt,
            status = status.name,
            lastUpdate = lastUpdate,
            outgoingUpdateTime = outgoingUpdateTime,
            lastSyncOfferId = lastSyncOfferId,
        )
    }

    private fun SsoSessionData.metadataToLocal(): List<SsoSessionMetadataLocal> {
        return metadata.entries.map { (key, value) ->
            SsoSessionMetadataLocal(
                sessionSharedSecretPublicKey = sharedSecretPublicKey.bytes.value,
                key = key.serialize(),
                value = value,
            )
        }
    }
}

private const val CUSTOM_PREFIX = "custom:"

private fun HandshakeMetadata.Key.serialize(): String = when (this) {
    HandshakeMetadata.Key.HostName -> "HostName"
    HandshakeMetadata.Key.HostVersion -> "HostVersion"
    HandshakeMetadata.Key.HostIcon -> "HostIcon"
    HandshakeMetadata.Key.PlatformType -> "PlatformType"
    HandshakeMetadata.Key.PlatformVersion -> "PlatformVersion"
    HandshakeMetadata.Key.Location -> "Location"
    is HandshakeMetadata.Key.Custom -> "$CUSTOM_PREFIX$name"
}

private fun String.toMetadataKey(): HandshakeMetadata.Key = when {
    this == "HostName" -> HandshakeMetadata.Key.HostName
    this == "HostVersion" -> HandshakeMetadata.Key.HostVersion
    this == "HostIcon" -> HandshakeMetadata.Key.HostIcon
    this == "PlatformType" -> HandshakeMetadata.Key.PlatformType
    this == "PlatformVersion" -> HandshakeMetadata.Key.PlatformVersion
    this == "Location" -> HandshakeMetadata.Key.Location
    startsWith(CUSTOM_PREFIX) -> HandshakeMetadata.Key.Custom(removePrefix(CUSTOM_PREFIX))
    else -> HandshakeMetadata.Key.Custom(this)
}
