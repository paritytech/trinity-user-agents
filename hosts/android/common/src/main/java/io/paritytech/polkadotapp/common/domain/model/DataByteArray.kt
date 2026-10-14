package io.paritytech.polkadotapp.common.domain.model

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.common.utils.compareTo
import kotlinx.serialization.Serializable

@Serializable
@TransientStruct
class DataByteArray(val value: ByteArraySerializable) {
    companion object {
        fun fromHex(hexString: HexString): DataByteArray {
            return hexString.fromHex().toDataByteArray()
        }

        fun empty(): DataByteArray {
            return DataByteArray(byteArrayOf())
        }

        fun <T> compareByBytes(unsigned: Boolean, selector: (T) -> ByteArray): Comparator<T> {
            return Comparator { a, b -> selector(a).compareTo(selector(b), unsigned) }
        }
    }

    override fun equals(other: Any?): Boolean {
        return this === other || other is DataByteArray && this.value contentEquals other.value
    }

    override fun hashCode(): Int = value.contentHashCode()

    override fun toString(): String = value.toHexString(withPrefix = true)
}

fun ByteArray.toDataByteArray() = DataByteArray(this)

fun String.hexToDataByteArray(): DataByteArray {
    return fromHex().toDataByteArray()
}

/**
 * Renders the bytes as UTF-8 text when they form a valid sequence, otherwise as a 0x-prefixed hex string.
 */
fun DataByteArray.asUtf8OrHex(): String = runCatching {
    value.decodeToString(throwOnInvalidSequence = true)
}.getOrElse {
    value.toHexString(withPrefix = true)
}
