package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

fun interface ProductFeedListener {
    suspend fun apply(chatId: ChatId, update: ProductFeedUpdateModel)
}

fun interface ProductFeedRegistration {
    /** Removes the listener. Idempotent. */
    fun cancel()
}

/**
 * The room screens that show a room whose messages the product keeps. The host stores none of the updates
 * of such a room: an update for a room that no screen shows changes only the page cache of the room, and the
 * screen asks the product again when it opens.
 */
interface ProductFeedListeners {
    /** Keeps [listener], a room screen, until the registration is cancelled. */
    fun register(chatId: ChatId, listener: ProductFeedListener): ProductFeedRegistration

    /** Gives the update to each listener of the room, in turn. False when no room screen takes it. */
    suspend fun deliver(chatId: ChatId, update: ProductFeedUpdateModel): Boolean
}
