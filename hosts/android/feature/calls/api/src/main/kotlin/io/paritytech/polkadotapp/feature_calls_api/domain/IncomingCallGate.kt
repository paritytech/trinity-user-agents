package io.paritytech.polkadotapp.feature_calls_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

interface IncomingCallGate {
    suspend fun shouldRing(chatId: ChatId, offerId: OfferId): Boolean
}
