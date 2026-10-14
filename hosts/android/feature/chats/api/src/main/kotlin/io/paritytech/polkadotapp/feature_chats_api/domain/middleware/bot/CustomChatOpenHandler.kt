package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

/**
 * Lets a chat extension take over opening its chat from the chat list instead of the host showing
 * the default chat feed. [handleOpen] returns true when the extension handled the open.
 */
fun interface CustomChatOpenHandler {
    suspend fun handleOpen(): Boolean
}
