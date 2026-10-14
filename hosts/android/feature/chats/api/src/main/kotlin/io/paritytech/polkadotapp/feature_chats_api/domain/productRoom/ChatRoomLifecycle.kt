package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * Opens and closes a product room while the room screen shows it. [open] holds the worker of the product
 * and sends `RoomOpened`. [close] sends `RoomClosed` and releases the worker. Each [open] that succeeds
 * gets one [close].
 */
interface ChatRoomLifecycle {
    /** Fails when the worker of the product cannot start. Then the room holds nothing. */
    suspend fun open(chatId: ChatId): Result<Unit>

    suspend fun close(chatId: ChatId)
}

/** The lifecycle of a room whose extension has none, such as a person chat. */
object NoChatRoomLifecycle : ChatRoomLifecycle {
    override suspend fun open(chatId: ChatId): Result<Unit> = Result.success(Unit)

    override suspend fun close(chatId: ChatId) = Unit
}
