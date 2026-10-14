package io.paritytech.polkadotapp.feature_sso_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeMetadata
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId

data class SsoSessionData(
    val sharedSecretPublicKey: X25519PublicKey,
    val statementStorePublicKey: EncodedPublicKey,
    val metadata: HandshakeMetadata,
    val addedAt: Long,
    val status: DeviceStatus,
    val lastUpdate: Long,
    val outgoingUpdateTime: Long?,
    val lastSyncOfferId: String?,
) : Identifiable {
    val id = SsoSessionId.fromSharedPubKey(sharedSecretPublicKey.bytes)

    val name: String get() = metadata.hostName.orEmpty()
    val icon: String get() = metadata.hostIcon.orEmpty()
    val hostVersion: String? get() = metadata.hostVersion
    val platformType: String? get() = metadata.platformType
    val platformVersion: String? get() = metadata.platformVersion

    override val identifier: String
        get() = sharedSecretPublicKey.bytes.value.toHexString()
}
