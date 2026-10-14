package io.paritytech.polkadotapp.common.utils.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decoder.ScaleDecoder
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encoder.ScaleEncoder
import io.paritytech.polkadotapp.common.domain.model.AccountEcdhKey
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * SCALE form of the on-chain chat encryption key: a fixed 65-byte container whose first byte is a
 * keypair-type marker, followed by the key and padding readers must ignore (RFC-0004 §4).
 *
 * The width predates X25519 — it is what an uncompressed P-256 point used to occupy — so the
 * container stays 65 bytes even though an X25519 key is 32.
 */
@Serializable(AccountEcdhKeyScaleSerializer::class)
sealed interface AccountEcdhKeyScale {
    data class X25519(val key: DataByteArray) : AccountEcdhKeyScale

    /** Retains the whole container so an unrecognised key re-encodes byte-for-byte. */
    data class Unknown(val rawValue: DataByteArray) : AccountEcdhKeyScale
}

fun AccountEcdhKeyScale.toDomain(): Result<AccountEcdhKey> = when (this) {
    is AccountEcdhKeyScale.X25519 -> X25519PublicKey.fromBytes(key).map(AccountEcdhKey::X25519)
    is AccountEcdhKeyScale.Unknown -> Result.success(AccountEcdhKey.Unknown(rawValue))
}

fun AccountEcdhKey.toScale(): AccountEcdhKeyScale = when (this) {
    is AccountEcdhKey.X25519 -> AccountEcdhKeyScale.X25519(key.bytes)
    is AccountEcdhKey.Unknown -> AccountEcdhKeyScale.Unknown(rawValue)
}

/** Raw 65-byte container, for the call/schema builders that take bytes rather than a serializer. */
fun AccountEcdhKeyScale.encodeOnChain(): ByteArray = Scale.encode(this) as ByteArray

object AccountEcdhKeyScaleSerializer : KSerializer<AccountEcdhKeyScale> {
    const val CONTAINER_SIZE_BYTES = 65

    private const val TYPE_X25519 = 0x00.toByte()

    private const val KEY_OFFSET = 1

    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("AccountEcdhKeyScale", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: AccountEcdhKeyScale) {
        require(encoder is ScaleEncoder) { "AccountEcdhKeyScale requires SCALE encoder" }

        val raw = when (value) {
            is AccountEcdhKeyScale.X25519 -> {
                val padding = ByteArray(CONTAINER_SIZE_BYTES - KEY_OFFSET - X25519PublicKey.SIZE_BYTES)
                byteArrayOf(TYPE_X25519) + value.key.value + padding
            }

            is AccountEcdhKeyScale.Unknown -> value.rawValue.value
        }

        encoder.encodeByteArray(raw)
    }

    override fun deserialize(decoder: Decoder): AccountEcdhKeyScale {
        require(decoder is ScaleDecoder) { "AccountEcdhKeyScale requires SCALE decoder" }

        val raw = decoder.decodeByteArray()
        require(raw.size == CONTAINER_SIZE_BYTES) {
            "On-chain encryption key must be $CONTAINER_SIZE_BYTES bytes, got ${raw.size}"
        }

        if (raw[0] != TYPE_X25519) return AccountEcdhKeyScale.Unknown(raw.toDataByteArray())

        // Padding after the key is deliberately not validated, per the RFC.
        val key = raw.copyOfRange(KEY_OFFSET, KEY_OFFSET + X25519PublicKey.SIZE_BYTES)

        return AccountEcdhKeyScale.X25519(key.toDataByteArray())
    }
}
