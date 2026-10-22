package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/** What the user does to a person chat of the chat product outside its room screen. */
interface PersonChatActions {
    /** Unblocks the contact of the room [chatId]. */
    suspend fun unblock(chatId: ChatId): Result<Unit>
}
