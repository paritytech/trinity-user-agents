package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatBuiltInMessageActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.collections.immutable.persistentListOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatMenuActionsProviderTest {
    private val provider = ChatMenuActionsProvider()

    private val canSend = ChatUserInputState.SendMessage(
        paymentState = ChatUserInputState.SendMessage.Payment.NotAvailable,
        attachmentsSupported = false,
    )
    private val hostStoreRoom = ChatRoomPolicy.Defaults
    private val productSourceRoom = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)

    @Test
    fun `a person chat never offers delete or retry`() {
        val actions = provider.getMessageActions(text(failed = true), canSend, roomPolicy = null)

        assertFalse(AllowedMessageMenuAction.Delete in actions)
        assertFalse(AllowedMessageMenuAction.Retry in actions)
    }

    @Test
    fun `a product room offers delete on every message`() {
        val incoming = provider.getMessageActions(text(direction = ChatMessageUiModel.Direction.INCOMING), canSend, hostStoreRoom)
        val outgoing = provider.getMessageActions(text(), canSend, hostStoreRoom)

        assertEquals(AllowedMessageMenuAction.Delete, incoming.last())
        assertEquals(AllowedMessageMenuAction.Delete, outgoing.last())
    }

    @Test
    fun `a failed message offers retry only in a product-source room`() {
        val hostStore = provider.getMessageActions(text(failed = true), canSend, hostStoreRoom)
        val productSource = provider.getMessageActions(text(failed = true), canSend, productSourceRoom)

        assertFalse(AllowedMessageMenuAction.Retry in hostStore)
        assertTrue(AllowedMessageMenuAction.Delete in hostStore)
        assertEquals(AllowedMessageMenuAction.Retry, productSource.first())
    }

    @Test
    fun `a failed attachment of the outbox offers retry in both room modes`() {
        val failed = text(failed = true).copy(id = "outbox:1")

        assertEquals(AllowedMessageMenuAction.Retry, provider.getMessageActions(failed, canSend, hostStoreRoom).first())
        assertEquals(AllowedMessageMenuAction.Retry, provider.getMessageActions(failed, canSend, productSourceRoom).first())
        assertFalse(AllowedMessageMenuAction.Retry in provider.getMessageActions(failed, canSend, roomPolicy = null))
    }

    @Test
    fun `a message that did not fail offers no retry`() {
        assertFalse(AllowedMessageMenuAction.Retry in provider.getMessageActions(text(), canSend, productSourceRoom))
    }

    @Test
    fun `rich text of a product can be copied but not edited`() {
        val actions = provider.getMessageActions(text(isMarkdown = true), canSend, hostStoreRoom)

        assertTrue(AllowedMessageMenuAction.Copy("hello") in actions)
        assertFalse(actions.any { it is AllowedMessageMenuAction.Edit })
    }

    @Test
    fun `the text of an actions message can be copied`() {
        val actions = provider.getMessageActions(actionsMessage(), canSend, hostStoreRoom)

        assertTrue(AllowedMessageMenuAction.Copy("Pick one") in actions)
    }

    @Test
    fun `an empty allowed reaction list removes the reactions`() {
        val noReactions = hostStoreRoom.copy(messageMenu = hostStoreRoom.messageMenu.copy(allowedReactions = emptyList()))
        val someReactions = hostStoreRoom.copy(messageMenu = hostStoreRoom.messageMenu.copy(allowedReactions = listOf("👍")))

        assertFalse(provider.canLeaveMenuReactions(canSend, noReactions))
        assertTrue(provider.canLeaveMenuReactions(canSend, someReactions))
        assertTrue(provider.canLeaveMenuReactions(canSend, roomPolicy = null))
        assertFalse(provider.canLeaveMenuReactions(ChatUserInputState.Nothing, hostStoreRoom))
    }

    @Test
    fun `extend built-in hides the hidden entries and adds the product entries in a section after them`() {
        val host = provider.getMessageActions(text(), canSend, hostStoreRoom)
        val menu = ChatMessageMenuModel.ExtendBuiltIn(
            actions = listOf(productAction("pin"), productAction("report", destructive = true), productAction("share")),
            hidden = setOf(ChatBuiltInMessageActionModel.EDIT),
        )

        val actions = provider.applyProductMenu(host, menu, icons = mapOf("share" to "icon"))

        assertEquals(host.filterNot { it is AllowedMessageMenuAction.Edit }, actions.take(host.size - 1))
        assertEquals(
            listOf(
                AllowedMessageMenuAction.Custom("pin", "pin", icon = null, isDestructive = false),
                AllowedMessageMenuAction.Custom("share", "share", icon = "icon", isDestructive = false),
                AllowedMessageMenuAction.Custom("report", "report", icon = null, isDestructive = true),
            ),
            actions.takeLast(3),
        )
    }

    @Test
    fun `custom shows only the product entries and no reactions`() {
        val host = provider.getMessageActions(text(), canSend, hostStoreRoom)
        val menu = ChatMessageMenuModel.Custom(listOf(productAction("pin")))

        assertEquals(
            listOf(AllowedMessageMenuAction.Custom("pin", "pin", icon = null, isDestructive = false)),
            provider.applyProductMenu(host, menu, icons = emptyMap()),
        )
        assertFalse(provider.canLeaveReactions(hostAllows = true, menu = menu))
    }

    @Test
    fun `no answer from the product shows the host menu`() {
        val host = provider.getMessageActions(text(), canSend, hostStoreRoom)

        assertEquals(host, provider.applyProductMenu(host, menu = null, icons = emptyMap()))
        assertTrue(provider.canLeaveReactions(hostAllows = true, menu = null))
    }

    @Test
    fun `hiding react removes the reactions`() {
        val hideReact = ChatMessageMenuModel.ExtendBuiltIn(emptyList(), hidden = setOf(ChatBuiltInMessageActionModel.REACT))
        val hideEdit = ChatMessageMenuModel.ExtendBuiltIn(emptyList(), hidden = setOf(ChatBuiltInMessageActionModel.EDIT))

        assertFalse(provider.canLeaveReactions(hostAllows = true, menu = hideReact))
        assertTrue(provider.canLeaveReactions(hostAllows = true, menu = hideEdit))
        assertFalse(provider.canLeaveReactions(hostAllows = false, menu = hideEdit))
    }

    private fun productAction(id: String, destructive: Boolean = false) = ChatProductActionModel(
        actionId = id,
        label = id,
        icon = null,
        destructive = destructive,
        handler = ChatActionHandlerModel.Callback,
    )

    private fun text(
        direction: ChatMessageUiModel.Direction = ChatMessageUiModel.Direction.OUTGOING,
        failed: Boolean = false,
        isMarkdown: Boolean = false,
    ) = ChatMessageUiModel.Text(
        id = "message-1",
        timestamp = 0L,
        direction = direction,
        status = if (failed) ChatMessageUiModel.Status.FAILED else ChatMessageUiModel.Status.SENT,
        origin = ChatMessageOrigin.User,
        text = "hello",
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false,
        isMarkdown = isMarkdown,
    )

    private fun actionsMessage() = ChatMessageUiModel.Actions(
        id = "message-2",
        timestamp = 0L,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.SENT,
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        text = "Pick one",
        buttons = persistentListOf(ChatMessageUiModel.Actions.ActionButton("a", "A")),
        layout = ChatMessageUiModel.Actions.ActionsLayout.COLUMN,
        reactions = persistentListOf(),
    )
}
