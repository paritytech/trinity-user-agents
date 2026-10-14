package io.paritytech.polkadotapp.tools_media_connection_api.domain

import kotlinx.coroutines.flow.Flow

typealias EncodedSdp = ByteArray
typealias EncodedIceCandidates = ByteArray

interface PeerChannelSignaling {
    suspend fun sendOffer(sdp: EncodedSdp)
    suspend fun sendAnswer(sdp: EncodedSdp)
    suspend fun sendIceCandidates(candidates: EncodedIceCandidates)
    fun subscribeIncomingIceCandidates(): Flow<EncodedIceCandidates>
    suspend fun awaitIncomingAnswer(): EncodedSdp
    suspend fun awaitIncomingOffer(): EncodedSdp
}
