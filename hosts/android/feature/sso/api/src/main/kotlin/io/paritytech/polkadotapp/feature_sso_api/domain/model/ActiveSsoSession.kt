package io.paritytech.polkadotapp.feature_sso_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

data class ActiveSsoSession(
    val id: String,
    val statementAccountId: AccountId,
    val encryptionPublicKey: X25519PublicKey,
    val name: String,
    val icon: String,
    val hostVersion: String?,
    val platformType: String?,
    val platformVersion: String?,
    val addedAt: Long,
    val status: DeviceStatus,
    val lastUpdate: Long,
)
