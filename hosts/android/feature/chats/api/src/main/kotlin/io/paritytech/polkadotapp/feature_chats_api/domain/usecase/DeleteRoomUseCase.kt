package io.paritytech.polkadotapp.feature_chats_api.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

interface DeleteRoomUseCase {
    suspend fun invoke(chatId: ChatId)

    /** Removes all the rooms and the bot personas of an extension, for example when the user uninstalls its product (02-DESIGN §5.5). */
    suspend fun deleteAllRoomsOf(extensionId: ChatExtensionId)
}
