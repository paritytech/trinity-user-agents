package io.paritytech.polkadotapp.common.utils.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decoder.ScaleDecoder
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encoder.ScaleEncoder
import io.paritytech.polkadotapp.common.utils.toBigEndianByteArray
import io.paritytech.polkadotapp.common.utils.toBigEndianInt
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * SCALE wrapper for the chain `BigEndianU32` newtype: a `u32` encoded as 4 raw big-endian bytes
 * (no length prefix). Used as the period key in storage maps with `Identity` hasher where the
 * raw bytes go into the storage key as-is.
 */
@JvmInline
@Serializable(BigEndianU32ScaleSerializer::class)
value class BigEndianU32Scale(val value: UInt)

object BigEndianU32ScaleSerializer : KSerializer<BigEndianU32Scale> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("BigEndianU32Scale", PrimitiveKind.INT)

    override fun serialize(encoder: Encoder, value: BigEndianU32Scale) {
        require(encoder is ScaleEncoder) { "BigEndianU32Scale requires SCALE encoder" }
        val array = value.value.toInt().toBigEndianByteArray()
        encoder.encodeByteArray(array)
    }

    override fun deserialize(decoder: Decoder): BigEndianU32Scale {
        require(decoder is ScaleDecoder) { "BigEndianU32Scale requires SCALE decoder" }
        val bytes = decoder.decodeByteArray()
        return BigEndianU32Scale(bytes.toBigEndianInt().toUInt())
    }
}
