package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatToolbarAction
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatMenuActionsProviderTest {
    private val provider = ChatMenuActionsProvider()

    private val productRoom = OpenChatRequest.ExistingChat(ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-01"))
    private val canSend = ChatUserInputState.SendMessage(
        paymentState = ChatUserInputState.SendMessage.Payment.NotAvailable,
        attachmentsSupported = true,
    )

    @Test
    fun `a room of the chat product shows the call buttons and the menu of an established person chat`() {
        val actions = provider.getToolbarActions(productRoom, canSend, addMenuIconForcibly = false, isPersonChat = true)

        assertEquals(listOf(ChatToolbarAction.AUDIO_CALL, ChatToolbarAction.VIDEO_CALL, ChatToolbarAction.MENU), actions)
    }

    @Test
    fun `a room of the chat product with a pending request shows the menu only, as a person chat does`() {
        for (state in listOf(ChatUserInputState.AcceptChatRequest, ChatUserInputState.WaitChatRequestApproval, ChatUserInputState.UnblockUser)) {
            val actions = provider.getToolbarActions(productRoom, state, addMenuIconForcibly = false, isPersonChat = true)

            assertEquals(listOf(ChatToolbarAction.MENU), actions)
        }
    }

    @Test
    fun `another product room keeps the toolbar of an extension`() {
        assertEquals(emptyList<ChatToolbarAction>(), provider.getToolbarActions(productRoom, canSend, addMenuIconForcibly = false))
        assertEquals(
            listOf(ChatToolbarAction.MENU),
            provider.getToolbarActions(productRoom, canSend, addMenuIconForcibly = true),
        )
    }
}
