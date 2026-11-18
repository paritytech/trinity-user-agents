package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf

/** Draws what an extension shows in place of the message input of a chat. */
interface CustomChatInputSlotRenderer {
    @Composable
    fun drawInputSlot()

    /**
     * Whether [drawInputSlot] calls [LocalChatInputSlotReady] once it has drawn its content.
     *
     * When such a renderer takes the slot from another, the chat keeps the one before on screen until then, so the slot
     * is never empty in between. A renderer that does not report is shown at once.
     */
    val reportsReady: Boolean get() = false
}

/** Called by a [CustomChatInputSlotRenderer] that [reports ready][CustomChatInputSlotRenderer.reportsReady] when its content is drawn. */
val LocalChatInputSlotReady = compositionLocalOf<() -> Unit> { {} }
