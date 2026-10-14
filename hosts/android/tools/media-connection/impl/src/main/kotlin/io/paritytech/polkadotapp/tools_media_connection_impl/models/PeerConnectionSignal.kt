package io.paritytech.polkadotapp.tools_media_connection_impl.models

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
sealed interface PeerConnectionSignal {
    @Serializable
    @EnumIndex(0)
    class Offer(val sdp: String) : PeerConnectionSignal

    @Serializable
    @EnumIndex(1)
    class Answer(val sdp: String) : PeerConnectionSignal

    @Serializable
    @EnumIndex(2)
    class IceCandidates(val candidates: List<PeerConnectionCandidate>) : PeerConnectionSignal
}

@Serializable
class PeerConnectionCandidate(
    val sdp: String,
    val sdpMLineIndex: UInt,
    val sdpMid: String?
)
