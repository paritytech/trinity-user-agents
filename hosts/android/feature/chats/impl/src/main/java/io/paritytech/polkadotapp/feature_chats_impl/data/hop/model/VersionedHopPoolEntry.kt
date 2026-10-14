package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray

@Serializable
sealed interface VersionedHopPoolEntry {
    @Serializable
    @EnumIndex(0)
    class V1(val payload: HopPoolEntryPayload) : VersionedHopPoolEntry

    companion object {
        fun decodePayload(bytes: ByteArray): HopPoolEntryPayload {
            return when (val versioned = BinaryScale.decodeFromByteArray<VersionedHopPoolEntry>(bytes)) {
                is V1 -> versioned.payload
            }
        }
    }
}

@Serializable
sealed interface HopPoolEntryPayload {
    @Serializable
    @EnumIndex(0)
    class Inline(val bytes: ByteArraySerializable) : HopPoolEntryPayload

    @Serializable
    @EnumIndex(1)
    class Chunked(val payload: HopChunkedPayload) : HopPoolEntryPayload
}

@Serializable
class HopChunkedPayload(
    val totalSize: ULong,
    val chunks: List<ByteArraySerializable>
)
