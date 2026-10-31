package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import kotlinx.collections.immutable.persistentListOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** The host database deletes no message of the user, so only a room whose product keeps the messages offers Delete. */
class ChatMenuActionsProviderTest {
    private val provider = ChatMenuActionsProvider()
    private val canSend = ChatUserInputState.SendMessage(ChatUserInputState.SendMessage.Payment.NotAvailable, attachmentsSupported = false)

    @Test
    fun `a message of the user offers Delete in a room whose product keeps the messages`() {
        val actions = provider.getMessageActions(message(ChatMessageUiModel.Direction.OUTGOING), canSend, deletesUserMessages = true)

        assertEquals(
            listOf(AllowedMessageMenuAction.Reply, AllowedMessageMenuAction.Copy("hi"), AllowedMessageMenuAction.Edit("hi"), AllowedMessageMenuAction.Delete),
            actions,
        )
    }

    @Test
    fun `no other message offers Delete`() {
        val outgoing = message(ChatMessageUiModel.Direction.OUTGOING)

        val incoming = message(ChatMessageUiModel.Direction.INCOMING)

        assertFalse(AllowedMessageMenuAction.Delete in provider.getMessageActions(outgoing, canSend, deletesUserMessages = false))
        assertFalse(AllowedMessageMenuAction.Delete in provider.getMessageActions(incoming, canSend, deletesUserMessages = true))
        assertFalse(AllowedMessageMenuAction.Delete in provider.getMessageActions(outgoing, ChatUserInputState.Nothing, deletesUserMessages = true))
    }

    private fun message(direction: ChatMessageUiModel.Direction) = ChatMessageUiModel.Text(
        id = "m1",
        timestamp = 1,
        direction = direction,
        status = ChatMessageUiModel.Status.READ,
        origin = ChatMessageOrigin.User,
        text = "hi",
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false,
    )
}
