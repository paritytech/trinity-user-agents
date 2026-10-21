package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignal
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignalCoder
import io.paritytech.polkadotapp.tools_media_connection_impl.models.PeerConnectionCandidate
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import org.webrtc.IceCandidate
import javax.inject.Inject

class RealPeerSignalCoder @Inject constructor() : PeerSignalCoder {
    private val sdpCoder = SdpCoder()
    private val serializer = ListSerializer(TextSignal.serializer())

    override fun encode(signal: PeerSignal): ByteArray {
        val signals = when (signal) {
            is PeerSignal.Offer -> setupSignals(signal.setup, TextSignal::Offer)
            is PeerSignal.Answer -> setupSignals(signal.setup, TextSignal::Answer)
            is PeerSignal.Candidates -> listOf(sdpCoder.decodeCandidates(signal.candidates).toSignal())
            PeerSignal.Closed -> listOf(TextSignal.Closed)
        }
        return BinaryScale.encodeToByteArray(serializer, signals)
    }

    override fun decode(signals: ByteArray): List<PeerSignal> {
        val decoded = BinaryScale.decodeFromByteArray(serializer, signals)
        val result = mutableListOf<PeerSignal>()
        var index = 0
        while (index < decoded.size) {
            val signal = decoded[index++]
            val following = (decoded.getOrNull(index) as? TextSignal.IceCandidates)?.candidates
            when (signal) {
                is TextSignal.Offer -> {
                    if (following != null) index++
                    result += PeerSignal.Offer(setupOf(signal.sdp, following))
                }

                is TextSignal.Answer -> {
                    if (following != null) index++
                    result += PeerSignal.Answer(setupOf(signal.sdp, following))
                }

                is TextSignal.IceCandidates -> result += PeerSignal.Candidates(sdpCoder.encodeCandidates(signal.candidates.map { it.toIce() }))
                TextSignal.Closed -> result += PeerSignal.Closed
            }
        }
        return result
    }

    private fun setupSignals(setup: ByteArray, signal: (String) -> TextSignal): List<TextSignal> {
        val decoded = sdpCoder.decodeSetup(setup)
        val candidates = decoded.candidates.takeIf { it.isNotEmpty() }?.toSignal()
        return listOfNotNull(signal(decoded.setupSdp), candidates)
    }

    private fun setupOf(sdp: String, candidates: List<PeerConnectionCandidate>?): ByteArray =
        sdpCoder.encodeSetup(SdpCoderSetup(sdp, candidates.orEmpty().map { it.toIce() }))

    private fun List<IceCandidate>.toSignal() =
        TextSignal.IceCandidates(map { PeerConnectionCandidate(it.sdp, it.sdpMLineIndex.toUInt(), it.sdpMid) })

    private fun PeerConnectionCandidate.toIce() = IceCandidate(sdpMid ?: "0", sdpMLineIndex.toInt(), sdp)
}

@Serializable
private sealed interface TextSignal {
    @Serializable
    @EnumIndex(0)
    class Offer(val sdp: String) : TextSignal

    @Serializable
    @EnumIndex(1)
    class Answer(val sdp: String) : TextSignal

    @Serializable
    @EnumIndex(2)
    class IceCandidates(val candidates: List<PeerConnectionCandidate>) : TextSignal

    @Serializable
    @EnumIndex(3)
    data object Closed : TextSignal
}
