package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import kotlinx.serialization.Serializable

@Serializable
sealed interface VersionedHandshakeOfferScale {
    @Serializable
    @EnumIndex(1)
    class V2(val value: HandshakeProposalV2Scale) : VersionedHandshakeOfferScale
}

@Serializable
class HandshakeProposalV2Scale(
    val device: DeviceScale,
    val metadata: List<MetadataEntryScale>,
)

@Serializable
class DeviceScale(
    @FixedLength(32)
    val statementAccountId: ByteArray,
    val encryptionPublicKey: X25519PublicKeyScale,
)

@Serializable
class MetadataEntryScale(
    val key: MetadataKeyScale,
    val value: String,
)

@Serializable
sealed interface MetadataKeyScale {
    @Serializable
    @EnumIndex(0)
    class Custom(val name: String) : MetadataKeyScale

    @Serializable
    @EnumIndex(1)
    data object HostName : MetadataKeyScale

    @Serializable
    @EnumIndex(2)
    data object HostVersion : MetadataKeyScale

    @Serializable
    @EnumIndex(3)
    data object HostIcon : MetadataKeyScale

    @Serializable
    @EnumIndex(4)
    data object PlatformType : MetadataKeyScale

    @Serializable
    @EnumIndex(5)
    data object PlatformVersion : MetadataKeyScale

    @Serializable
    @EnumIndex(6)
    data object Location : MetadataKeyScale
}
