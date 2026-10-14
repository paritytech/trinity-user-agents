package io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

/** SCALE wire format for WebRTC signalling. [Reconnected] asks the peer to re-issue an offer. */
@Serializable
sealed interface SignalingMessage {
    @Serializable
    @EnumIndex(0)
    data object Reconnected : SignalingMessage

    @Serializable
    @EnumIndex(1)
    class Offer(val sdp: ByteArray) : SignalingMessage

    @Serializable
    @EnumIndex(2)
    class Answer(val sdp: ByteArray) : SignalingMessage

    @Serializable
    @EnumIndex(3)
    class IceCandidates(val candidates: ByteArray) : SignalingMessage
}
