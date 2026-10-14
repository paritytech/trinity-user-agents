package io.paritytech.polkadotapp.feature_sso_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

class HandshakeOffer(
    val device: HandshakeDevice,
    val metadata: HandshakeMetadata,
)

class HandshakeDevice(
    val statementAccountId: AccountId,
    val encryptionPublicKey: X25519PublicKey,
)

class HandshakeMetadata(val entries: Map<Key, String>) {
    sealed interface Key {
        data class Custom(val name: String) : Key
        data object HostName : Key
        data object HostVersion : Key
        data object HostIcon : Key
        data object PlatformType : Key
        data object PlatformVersion : Key
        data object Location : Key
    }

    val hostName: String? get() = entries[Key.HostName]
    val hostVersion: String? get() = entries[Key.HostVersion]
    val hostIcon: String? get() = entries[Key.HostIcon]
    val platformType: String? get() = entries[Key.PlatformType]
    val platformVersion: String? get() = entries[Key.PlatformVersion]
    val location: String? get() = entries[Key.Location]
}
