package io.paritytech.polkadotapp.feature_products_api.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import kotlinx.serialization.Serializable

@Serializable
sealed class RingLocationJunctionScale {
    @Serializable
    @EnumIndex(0)
    data class PalletInstance(val index: UByte) : RingLocationJunctionScale()

    @Serializable
    @EnumIndex(1)
    data class CollectionId(val bytes: DataByteArray) : RingLocationJunctionScale()
}

/**
 * Shared by the SSO wire and the RFC-0024 registry table, so a ring persisted locally and a ring
 * received from a Host compare as the same bytes.
 */
@Serializable
data class RingLocationScale(
    // chainId is GenesisHash = [u8; 32] on the wire: 32 raw bytes, no length prefix.
    @FixedLength(32)
    val chainId: ByteArray,
    val junctions: List<RingLocationJunctionScale>,
)

fun RingLocation.toScale(): RingLocationScale =
    RingLocationScale(chainId.value, junctions.map { it.toScale() })

fun RingLocationScale.toDomain(): RingLocation =
    RingLocation(chainId.toDataByteArray(), junctions.map { it.toDomain() })

private fun RingLocationJunction.toScale(): RingLocationJunctionScale = when (this) {
    is RingLocationJunction.PalletInstance -> RingLocationJunctionScale.PalletInstance(index)
    is RingLocationJunction.CollectionId -> RingLocationJunctionScale.CollectionId(bytes)
}

private fun RingLocationJunctionScale.toDomain(): RingLocationJunction = when (this) {
    is RingLocationJunctionScale.PalletInstance -> RingLocationJunction.PalletInstance(index)
    is RingLocationJunctionScale.CollectionId -> RingLocationJunction.CollectionId(bytes)
}
