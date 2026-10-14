package io.paritytech.polkadotapp.common.domain.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import kotlinx.serialization.Serializable

/**
 * Wire form of an X25519 public key: 32 raw bytes, no length prefix.
 *
 * Deliberately a plain class over a bare [ByteArray]. A `@JvmInline value class` does not work here:
 * its inline descriptor drops [FixedLength] and the encoder falls back to a compact length prefix,
 * yielding 33 bytes. A custom serializer cannot fix that either — `BinaryScaleEncoder` exposes no
 * raw byte-array hook, so [FixedLength] on a property is the only mechanism the codec offers.
 */
@Serializable
class X25519PublicKeyScale(
    @FixedLength(X25519PublicKey.SIZE_BYTES)
    val bytes: ByteArray,
)

fun X25519PublicKey.toScale(): X25519PublicKeyScale = X25519PublicKeyScale(bytes.value)

fun X25519PublicKeyScale.toDomain(): Result<X25519PublicKey> = X25519PublicKey.fromBytes(bytes.toDataByteArray())
