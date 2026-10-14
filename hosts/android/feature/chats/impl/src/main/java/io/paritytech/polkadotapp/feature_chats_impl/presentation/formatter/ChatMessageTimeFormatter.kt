package io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter

import androidx.compose.runtime.staticCompositionLocalOf
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor

val LocalChatMessageTimeFormatter = staticCompositionLocalOf<ChatMessageTimeFormatter> {
    noLocalProvidedFor("ChatMessageTimeFormatter")
}

interface ChatMessageTimeFormatter {
    /**
     * Formats time for message bubbles and edit-history rows. Always returns "HH:mm" (24h).
     */
    fun formatMessageTime(time: Timestamp): String

    /**
     * Formats time for chat list items.
     * Returns: "Now", "{n}m", "{n}h", "Yesterday", weekday name, "dd.MM", or "dd.MM.yyyy"
     */
    fun formatChatListTime(time: Timestamp, relativeTo: Timestamp = System.currentTimeMillis()): String

    companion object {
        fun mocked(): ChatMessageTimeFormatter = MockedChatMessageTimeFormatter()
    }
}

private class MockedChatMessageTimeFormatter : ChatMessageTimeFormatter {
    override fun formatMessageTime(time: Timestamp): String = "14:30"
    override fun formatChatListTime(time: Timestamp, relativeTo: Timestamp): String = "Yesterday"
}
