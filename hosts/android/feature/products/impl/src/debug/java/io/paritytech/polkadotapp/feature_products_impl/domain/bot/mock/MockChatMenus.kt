package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatMedia.Glyph
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatBuiltInMessageAction
import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMenu
import uniffi.truapi.ChatParticipant
import uniffi.truapi.ChatParticipantEntry
import uniffi.truapi.ChatParticipantMenu
import uniffi.truapi.ChatProductAction

/** The menus that the mock product answers: both menu kinds, hidden built-in entries, icons, destructive entries and both handlers. */
class MockChatMenus(private val media: MockChatMedia) {
    object Action {
        const val THREAD = "thread"
        const val PIN = "pin"
        const val REACT = "react"
        const val HIDE = "hide"
        const val REFRESH = "refresh"
        const val MENTION = "mention"
        const val MUTE = "mute"
        const val ABOUT = "about"
    }

    fun messageMenu(message: ChatMessage): ChatMessageMenu {
        if (message.origin.participantId == MockChatShowcase.Participant.USER) {
            return ChatMessageMenu.ExtendBuiltIn(listOf(entry(Action.PIN, "Pin", Glyph.PIN)), hidden = emptyList())
        }
        return when (message.body) {
            is ChatMessageBody.Custom -> ChatMessageMenu.Custom(
                listOf(
                    entry(Action.REFRESH, "Draw again", Glyph.REFRESH),
                    entry("docs", "Open polkadot.com", Glyph.LINK, handler = ChatActionHandler.Deeplink("https://polkadot.com")),
                ),
            )
            is ChatMessageBody.Notice -> ChatMessageMenu.Custom(emptyList())
            is ChatMessageBody.Text,
            is ChatMessageBody.RichText,
            is ChatMessageBody.Actions,
            -> ChatMessageMenu.ExtendBuiltIn(
                listOf(
                    entry(Action.THREAD, "Start a thread", Glyph.THREAD),
                    entry(Action.REACT, "Alice reacts 🎉", Glyph.PARTY),
                    entry(Action.HIDE, "Hide for everyone", Glyph.TRASH, destructive = true),
                ),
                hidden = listOf(ChatBuiltInMessageAction.EDIT, ChatBuiltInMessageAction.DELETE),
            )
        }
    }

    fun participantMenu(entry: ChatParticipantEntry): ChatParticipantMenu = when {
        entry.participantId.startsWith("bot:") -> ChatParticipantMenu.Custom(listOf(entry(Action.ABOUT, "About this persona", Glyph.INFO)))
        entry.participant is ChatParticipant.Custom -> ChatParticipantMenu.ExtendBuiltIn(
            listOf(entry(Action.MENTION, "Mention", Glyph.AT), entry(Action.MUTE, "Mute in this room", Glyph.MUTE, destructive = true)),
        )
        else -> ChatParticipantMenu.ExtendBuiltIn(emptyList())
    }

    private fun entry(actionId: String, label: String, glyph: Glyph, destructive: Boolean = false, handler: ChatActionHandler = ChatActionHandler.Callback) =
        ChatProductAction(actionId, label, MockChatShowcase.png(media.icon(glyph)), destructive, handler)
}
