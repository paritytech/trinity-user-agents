package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32

/**
 * RFC-0022 `Either<u32, [u8; 32]>` as it travels over the JS bridge: a JSON number is a plain index,
 * a hex string is a raw 32-byte selector. Serialized by [DerivationIndexWireAdapter].
 */
sealed interface DerivationIndexWire {
    data class Plain(val index: UInt) : DerivationIndexWire

    data class Raw(val bytes: DataByteArray) : DerivationIndexWire
}

fun DerivationIndexWire.toDomain(): Result<DerivationIndex32> = when (this) {
    is DerivationIndexWire.Plain -> Result.success(DerivationIndex32.fromUInt(index))
    is DerivationIndexWire.Raw -> DerivationIndex32.fromBytes(bytes)
}

/**
 * Re-narrows to [DerivationIndexWire.Plain] whenever the selector carries the index magic, so a
 * plain index sent by a product survives the round trip as a number rather than coming back as hex.
 */
fun DerivationIndex32.toWire(): DerivationIndexWire {
    val plainIndex = asUIntOrNull()

    return if (plainIndex != null) DerivationIndexWire.Plain(plainIndex) else DerivationIndexWire.Raw(bytes)
}
