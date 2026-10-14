package io.paritytech.polkadotapp.feature_calls_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow

typealias EncodedSpd = ByteArray
typealias EncodedIceCandidates = ByteArray
typealias OfferId = String

data class IncomingOffer(val sdp: EncodedSpd, val withVideo: Boolean)

interface ExternalCallSignaling {
    suspend fun sendOffer(offerId: OfferId, chatId: ChatId, sdp: EncodedSpd, withVideo: Boolean)
    suspend fun sendAnswer(offerId: OfferId, chatId: ChatId, sdp: EncodedSpd)
    suspend fun sendIceCandidates(offerId: OfferId, chatId: ChatId, candidates: EncodedIceCandidates)

    fun subscribeIncomingIceCandidates(chatId: ChatId, offerId: OfferId): Flow<EncodedIceCandidates>

    suspend fun awaitIncomingAnswer(offerId: OfferId, chatId: ChatId): EncodedSpd
    suspend fun awaitIncomingOffer(offerId: OfferId): IncomingOffer

    suspend fun sendCloseSignal(offerId: OfferId, chatId: ChatId)
    fun subscribeIncomingCloseSignal(offerId: OfferId, chatId: ChatId): Flow<Unit>

    fun observeOfferReadStatus(offerId: OfferId): Flow<Boolean>
}
