package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputPaymentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy.BuiltInMessageAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy.ListBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy.ListPreview
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatRoomPolicyTest {
    private val icon = ChatResourceModel.Inline("image/png", byteArrayOf(1, 2, 3).toDataByteArray())

    @Test
    fun `a room without declarations gets every default of the design`() {
        val policy = ChatRoomPolicy.from(null)

        assertEquals(ChatRoomPolicy.Defaults, policy)
        assertEquals(ChatRoomPolicy.MessageStore.HOST, policy.messageStore)
        assertEquals(ListPreview.LastMessage, policy.listPreview)
        assertEquals(ListBadge.HostUnreadCount, policy.listBadge)
        assertEquals(ChatRoomPolicy.ListOrder(isPinned = false, timestamp = null), policy.listOrder)
        assertEquals(BuiltInMessageAction.entries.toSet(), policy.messageMenu.builtIns)
        assertNull(policy.messageMenu.allowedReactions)
        assertFalse(policy.offersReactionDetails)
        assertFalse(policy.reportsSeen)
        assertTrue(policy.toolbar.isEmpty())
        assertNull(policy.placeholder)
        assertTrue(policy.suggestions.isEmpty())
        assertTrue(policy.commands.isEmpty())
        assertTrue(policy.productQueries.isEmpty())
        assertTrue(policy.hostAppliesUserActions)
    }

    @Test
    fun `the default footer is the input with photo and video and without file and payment`() {
        val footer = ChatRoomPolicy.Defaults.footer as ChatRoomPolicy.Footer.TextInput

        assertTrue(footer.input.allowsPhoto)
        assertTrue(footer.input.allowsVideo)
        assertFalse(footer.input.allowsFile)
        assertFalse(footer.input.allowsPayment)
        assertNull(footer.input.placeholder)
    }

    @Test
    fun `the declared configuration of the host default gives the defaults`() {
        assertEquals(ChatRoomPolicy.Defaults.copy(configGeneration = 1), ChatRoomPolicy.from(declarations()))
    }

    @Test
    fun `empty declarations give the defaults`() {
        assertEquals(ChatRoomPolicy.Defaults, ChatRoomPolicy.from(ChatRoomDeclarations.EMPTY))
        assertEquals(ChatRoomPolicy.Defaults.copy(configGeneration = 1), ChatRoomPolicy.from(declarations(config = null)))
    }

    @Test
    fun `the policy carries the configuration generation, so a screen asks its questions again`() {
        assertEquals(7L, ChatRoomPolicy.from(declarations().copy(configGeneration = 7)).configGeneration)
    }

    @Test
    fun `retry is offered only when the product keeps the messages`() {
        assertFalse(ChatRoomPolicy.Defaults.offersRetry)
        assertTrue(ChatRoomPolicy.Defaults.offersDelete)

        val productSource = ChatRoomPolicy.from(declarations(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true)))

        assertEquals(ChatRoomPolicy.MessageStore.PRODUCT, productSource.messageStore)
        assertTrue(productSource.offersRetry)
        assertFalse(productSource.hostAppliesUserActions)
    }

    @Test
    fun `the configuration flags select the product queries and the reaction details`() {
        val config = ChatRoomConfigModel.HOST_DEFAULT.copy(
            reactionDetails = true,
            participants = true,
            messageActions = true,
            participantActions = false,
            reportSeen = true,
            allowedReactions = listOf("👍", "🔥"),
            placeholder = "Ask me anything",
        )

        val policy = ChatRoomPolicy.from(declarations(config = config))

        assertTrue(policy.offersReactionDetails)
        assertTrue(policy.reportsSeen)
        assertEquals(listOf("👍", "🔥"), policy.messageMenu.allowedReactions)
        assertEquals("Ask me anything", policy.placeholder)
        assertEquals(
            setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS, ChatRoomPolicy.ProductQuery.MESSAGE_ACTIONS),
            policy.productQueries,
        )
    }

    @Test
    fun `the preview sets the text, the pin, the timestamp and the badge`() {
        val policy = ChatRoomPolicy.from(
            declarations(
                preview = preview(
                    order = Order.PinToTop,
                    timestamp = 5_000L,
                    badge = ChatRoomBadgeModel.Count(3),
                    text = "Bot is typing",
                    ongoing = true,
                )
            )
        )

        assertEquals(ListPreview.Text("Bot is typing", ongoing = true), policy.listPreview)
        assertEquals(ChatRoomPolicy.ListOrder(isPinned = true, timestamp = 5_000L), policy.listOrder)
        assertEquals(ListBadge.Count(3), policy.listBadge)
        assertTrue(policy.replacesHostBadge)
    }

    @Test
    fun `a preview without a badge keeps the host count and a zero timestamp reads as none`() {
        val policy = ChatRoomPolicy.from(declarations(preview = preview(timestamp = 0L, text = "Hello")))

        assertEquals(ListBadge.HostUnreadCount, policy.listBadge)
        assertFalse(policy.replacesHostBadge)
        assertNull(policy.listOrder.timestamp)
    }

    @Test
    fun `a hidden badge and an image badge replace the host count`() {
        val hidden = ChatRoomPolicy.from(declarations(preview = preview(badge = ChatRoomBadgeModel.Hidden)))
        val image = ChatRoomPolicy.from(declarations(preview = preview(badge = ChatRoomBadgeModel.Image(icon))))

        assertEquals(ListBadge.Hidden, hidden.listBadge)
        assertEquals(ListBadge.Image(icon), image.listBadge)
        assertTrue(hidden.replacesHostBadge)
        assertTrue(image.replacesHostBadge)
    }

    @Test
    fun `a negative count reads as zero`() {
        val policy = ChatRoomPolicy.from(declarations(preview = preview(badge = ChatRoomBadgeModel.Count(-4))))

        assertEquals(ListBadge.Count(0), policy.listBadge)
    }

    @Test
    fun `the metadata gives the description and the avatar`() {
        val policy = ChatRoomPolicy.from(declarations(description = "Weather for your city", avatar = icon))

        assertEquals("Weather for your city", policy.description)
        assertEquals(icon, policy.avatar)
    }

    @Test
    fun `the declared footer replaces the default footer`() {
        val input = ChatInputConfigModel(
            placeholder = "Say hi",
            photo = false,
            video = false,
            file = true,
            payment = ChatInputPaymentModel.Enabled(ByteArray(32).toDataByteArray()),
        )

        val withInput = ChatRoomPolicy.from(declarations(footer = ChatRoomFooterModel.Input(input)))
        val empty = ChatRoomPolicy.from(declarations(footer = ChatRoomFooterModel.Empty))

        assertEquals(
            ChatRoomPolicy.Footer.TextInput(
                ChatRoomPolicy.Input(
                    allowsPhoto = false,
                    allowsVideo = false,
                    allowsFile = true,
                    allowsPayment = true,
                    placeholder = "Say hi",
                )
            ),
            withInput.footer,
        )
        assertEquals(ChatRoomPolicy.Footer.Empty, empty.footer)
    }
}
