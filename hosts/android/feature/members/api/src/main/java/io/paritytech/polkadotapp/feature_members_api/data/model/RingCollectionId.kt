package io.paritytech.polkadotapp.feature_members_api.data.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class RingCollectionId(val value: DataByteArray) {
    init {
        require(value.value.size == SIZE_BYTES) {
            "RingCollectionId must be exactly $SIZE_BYTES bytes, was ${value.value.size}"
        }
    }

    companion object {
        private const val SIZE_BYTES = 32
        private const val SPACE_BYTE: Byte = ' '.code.toByte()

        fun paddedBytes(prefix: ByteArray, padByte: Byte = 0): RingCollectionId {
            require(prefix.size <= SIZE_BYTES) {
                "Prefix must be at most $SIZE_BYTES bytes, was ${prefix.size}"
            }
            val bytes = ByteArray(SIZE_BYTES) { padByte }
            prefix.copyInto(bytes)
            return RingCollectionId(bytes.toDataByteArray())
        }

        fun paddedString(prefix: String, padByte: Byte = SPACE_BYTE): RingCollectionId {
            return paddedBytes(prefix.toByteArray(), padByte)
        }
    }
}
