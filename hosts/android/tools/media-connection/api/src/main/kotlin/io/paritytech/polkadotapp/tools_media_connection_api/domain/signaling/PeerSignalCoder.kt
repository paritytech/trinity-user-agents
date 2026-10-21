package io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling

import io.paritytech.polkadotapp.tools_media_connection_api.domain.EncodedIceCandidates
import io.paritytech.polkadotapp.tools_media_connection_api.domain.EncodedSdp

/** One signal of a call, with the offer and answer in their encoded setup form. */
sealed interface PeerSignal {
    class Offer(val setup: EncodedSdp) : PeerSignal

    class Answer(val setup: EncodedSdp) : PeerSignal

    class Candidates(val candidates: EncodedIceCandidates) : PeerSignal

    data object Closed : PeerSignal
}

/**
 * Converts [PeerSignal]s to the SCALE `Vec<PeerConnectionSignal>` the iOS app and the chat product carry, with the SDP
 * as text, and back.
 */
interface PeerSignalCoder {
    fun encode(signal: PeerSignal): ByteArray

    /** Throws for bytes that are not such a list. An offer or answer takes the candidates that follow it. */
    fun decode(signals: ByteArray): List<PeerSignal>
}
