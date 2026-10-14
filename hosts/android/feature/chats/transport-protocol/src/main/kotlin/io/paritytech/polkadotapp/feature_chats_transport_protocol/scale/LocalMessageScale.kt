package io.paritytech.polkadotapp.feature_chats_transport_protocol.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import kotlinx.serialization.Serializable

/** `LocalMessage` from the multi-device chat spec. [peerId] is the chat partner. */
@Serializable
class LocalMessageScale(
    val remote: ChatMessageStatement,
    @FixedLength(32)
    val peerId: ByteArray,
    val status: LocalStatusScale,
    val order: ULong,
)

@Serializable
sealed interface LocalStatusScale {
    @Serializable
    @EnumIndex(0)
    class Outgoing(val status: OutgoingStatusScale) : LocalStatusScale

    @Serializable
    @EnumIndex(1)
    class Incoming(val status: IncomingStatusScale) : LocalStatusScale
}

@Serializable
enum class OutgoingStatusScale {
    @EnumIndex(0)
    NEW,

    @EnumIndex(1)
    SENT,

    @EnumIndex(2)
    DELIVERED,
}

@Serializable
enum class IncomingStatusScale {
    @EnumIndex(0)
    NEW,

    @EnumIndex(1)
    SEEN,
}
