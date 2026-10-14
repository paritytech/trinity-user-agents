package io.paritytech.polkadotapp.feature_products_impl.data.mappers

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.model.RingVrfKeyRegistrationLocal
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.scale.RingLocationScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.toDomain
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistration
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

fun RingVrfKeyRegistration.toLocal(): RingVrfKeyRegistrationLocal = RingVrfKeyRegistrationLocal(
    ownerProductId = handle.productId,
    derivationIndex = handle.index.bytes.value,
    ringLocation = ring.encode(),
    publicKey = publicKey.value,
    registeredAt = System.currentTimeMillis(),
)

fun RingVrfKeyRegistrationLocal.toDomain(): Result<RingVrfKeyRegistration> {
    return DerivationIndex32.fromBytes(derivationIndex.toDataByteArray())
        .mapCatching { index ->
            RingVrfKeyRegistration(
                handle = ProductAccountId(ownerProductId, index),
                ring = ringLocation.decodeRing(),
                publicKey = publicKey.toDataByteArray(),
            )
        }
}

private fun RingLocation.encode(): ByteArray = BinaryScale.encodeToByteArray(toScale())

private fun ByteArray.decodeRing(): RingLocation =
    BinaryScale.decodeFromByteArray<RingLocationScale>(this).toDomain()
