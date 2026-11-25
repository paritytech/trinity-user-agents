package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable

/** Draws what an extension shows in place of the message input of a chat. */
interface CustomChatInputSlotRenderer {
    @Composable
    fun drawInputSlot()
}
