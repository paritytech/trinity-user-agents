package io.paritytech.polkadotapp.feature_calls_impl.signaling

import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.EncodedIceCandidates
import io.paritytech.polkadotapp.tools_media_connection_api.domain.EncodedSdp
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelSignaling
import kotlinx.coroutines.flow.Flow

fun ExternalCallSignaling.asPeerChannelSignaling(
    chatId: ChatId,
    offerId: OfferId,
    withVideo: Boolean,
): PeerChannelSignaling = object : PeerChannelSignaling {
    override suspend fun sendOffer(sdp: EncodedSdp) = sendOffer(offerId, chatId, sdp, withVideo)
    override suspend fun sendAnswer(sdp: EncodedSdp) = sendAnswer(offerId, chatId, sdp)
    override suspend fun sendIceCandidates(candidates: EncodedIceCandidates) = sendIceCandidates(offerId, chatId, candidates)
    override fun subscribeIncomingIceCandidates(): Flow<EncodedIceCandidates> = subscribeIncomingIceCandidates(chatId, offerId)
    override suspend fun awaitIncomingAnswer(): EncodedSdp = awaitIncomingAnswer(offerId, chatId)
    override suspend fun awaitIncomingOffer(): EncodedSdp = awaitIncomingOffer(offerId).sdp
}
