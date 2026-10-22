package io.paritytech.polkadotapp.feature_chats_api.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

interface DeleteRoomUseCase {
    suspend fun invoke(chatId: ChatId)
}
