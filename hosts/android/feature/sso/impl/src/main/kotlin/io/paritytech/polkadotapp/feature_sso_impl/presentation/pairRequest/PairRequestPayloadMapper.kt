package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeDevice
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeMetadata
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer

fun HandshakeOffer.toPayload(): PairRequestPayload {
    return PairRequestPayload(
        statementAccountId = device.statementAccountId.value,
        encryptionPublicKey = device.encryptionPublicKey.bytes.value,
        metadata = metadata.entries.mapKeys { (key, _) -> key.toPayload() },
    )
}

fun PairRequestPayload.toDomain(): HandshakeOffer {
    return HandshakeOffer(
        device = HandshakeDevice(
            statementAccountId = statementAccountId.intoAccountId(),
            encryptionPublicKey = encryptionPublicKey.requireX25519PublicKey(),
        ),
        metadata = HandshakeMetadata(
            entries = metadata.mapKeys { (key, _) -> key.toDomain() },
        ),
    )
}

private fun HandshakeMetadata.Key.toPayload(): MetadataKey = when (this) {
    is HandshakeMetadata.Key.Custom -> MetadataKey.Custom(name)
    HandshakeMetadata.Key.HostName -> MetadataKey.HostName
    HandshakeMetadata.Key.HostVersion -> MetadataKey.HostVersion
    HandshakeMetadata.Key.HostIcon -> MetadataKey.HostIcon
    HandshakeMetadata.Key.PlatformType -> MetadataKey.PlatformType
    HandshakeMetadata.Key.PlatformVersion -> MetadataKey.PlatformVersion
    HandshakeMetadata.Key.Location -> MetadataKey.Location
}

private fun MetadataKey.toDomain(): HandshakeMetadata.Key = when (this) {
    is MetadataKey.Custom -> HandshakeMetadata.Key.Custom(name)
    MetadataKey.HostName -> HandshakeMetadata.Key.HostName
    MetadataKey.HostVersion -> HandshakeMetadata.Key.HostVersion
    MetadataKey.HostIcon -> HandshakeMetadata.Key.HostIcon
    MetadataKey.PlatformType -> HandshakeMetadata.Key.PlatformType
    MetadataKey.PlatformVersion -> HandshakeMetadata.Key.PlatformVersion
    MetadataKey.Location -> HandshakeMetadata.Key.Location
}
