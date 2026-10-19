package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatActionLayout
import uniffi.truapi.ChatMediaItem
import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMenu
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ChatParticipant
import uniffi.truapi.ColorToken
import uniffi.truapi.Modifier
import uniffi.truapi.RendererNode
import uniffi.truapi.TypographyStyle

// The chat showcase exists so that a tester sees every kind of thing that a product can put in a room. These tests fail
// when the wire gains a kind that the showcase does not show, so the showcase grows with the wire. Each `kind` is an
// exhaustive `when`: a new case stops the build here first.
class MockChatShowcaseTest {
    private val media = MockChatMedia(
        pictures = List(4) { byteArrayOf(1) },
        video = byteArrayOf(2),
        document = byteArrayOf(3),
        icon = { byteArrayOf(4) },
        avatar = { _, _ -> byteArrayOf(5) },
    )
    private val now = 1_800_000_000_000uL
    private val messages = MockChatShowcaseMessages.showcase(media, now)

    @Test
    fun `every body kind is shown`() {
        assertEquals(BodyKind.entries.toSet(), messages.map { kind(it.body) }.toSet())
    }

    @Test
    fun `every status is shown`() {
        assertEquals(ChatMessageStatus.entries.toSet(), messages.map { it.status }.toSet())
    }

    @Test
    fun `every media kind is shown`() {
        val items = messages.flatMap { (it.body as? ChatMessageBody.RichText)?.media.orEmpty() }

        assertEquals(MediaKind.entries.toSet(), items.map(::kind).toSet())
    }

    @Test
    fun `every participant kind is listed`() {
        assertEquals(ParticipantKind.entries.toSet(), MockChatShowcase.participants(media).map { kind(it.participant) }.toSet())
    }

    // The room opens on its newest page, so the first page must hold the whole showcase and not only history.
    @Test
    fun `the first page of the host holds the whole showcase`() {
        val firstPage = MockChatConversation(messages).page(ChatPageAnchor.Before(null), size = 50)
        val showcase = messages.map(ChatMessage::messageId).filterNot { it.startsWith("m-history") }

        assertTrue(firstPage.messages.map(ChatMessage::messageId).containsAll(showcase))
        assertNotNull(firstPage.next)
    }

    @Test
    fun `the meta and the replies name messages of the room`() {
        val ids = messages.map(ChatMessage::messageId).toSet()
        val meta = MockChatShowcaseMessages.showcaseMeta(now)

        assertTrue(meta.all { (messageId, _) -> messageId in ids })
        assertTrue(meta.any { it.second.thread != null } && meta.any { it.second.branch != null } && meta.any { it.second.reasoning != null })
        assertTrue(messages.mapNotNull { it.replyTo }.let { replies -> replies.isNotEmpty() && replies.all { it in ids } })
    }

    @Test
    fun `the trees use every node, typography, colour and button variant`() {
        val usage = TreeUsage()
        listOf(MockChatRenderTrees.card("", 0), MockChatRenderTrees.counter(0), MockChatRenderTrees.footer()).forEach(usage::visit)

        assertEquals(NodeKind.entries.toSet(), usage.nodes)
        assertEquals(TypographyStyle.entries.toSet(), usage.typography)
        assertEquals(ColorToken.entries.toSet(), usage.colours)
        assertEquals(ButtonVariant.entries.toSet(), usage.buttons)
    }

    @Test
    fun `the menus use both kinds and both handlers`() {
        val menus = MockChatMenus(media)
        val answers = listOf("m-user-sent", "m-card", "m-text").map { id -> menus.messageMenu(messages.first { it.messageId == id }) }
        val actions = answers.flatMap {
            when (it) {
                is ChatMessageMenu.Custom -> it.actions
                is ChatMessageMenu.ExtendBuiltIn -> it.actions
            }
        }

        assertTrue(answers.any { it is ChatMessageMenu.Custom })
        assertTrue(answers.any { it is ChatMessageMenu.ExtendBuiltIn && it.hidden.isNotEmpty() })
        assertTrue(actions.any { it.destructive })
        assertTrue(actions.any { it.handler is ChatActionHandler.Deeplink })
    }

    @Test
    fun `a press draws the body that the product keeps again`() {
        val counter = MockChatBody.Counter(0)
            .after(MockChatRenderTrees.Action.INCREMENT, typed = null)
            .after(MockChatRenderTrees.Action.INCREMENT, typed = null)
        val card = MockChatBody.Card(typed = "", primaryPresses = 0).after(MockChatRenderTrees.Action.TYPED, typed = "hello")

        assertEquals(MockChatBody.Counter(2), counter)
        assertEquals(MockChatBody.Counter(0), counter.after(MockChatRenderTrees.Action.RESET, typed = null))
        assertEquals(MockChatBody.Card(typed = "hello", primaryPresses = 0), card)
        assertEquals(MockChatBody.Card(typed = "", primaryPresses = 0), card.initial())
    }

    private enum class BodyKind { TEXT, MARKDOWN, RICH_TEXT_WITH_MEDIA, ACTIONS_COLUMN, ACTIONS_GRID, CUSTOM, NOTICE }

    private enum class MediaKind { IMAGE, VIDEO, FILE }

    private enum class ParticipantKind { PERSON, LITE_PERSON, CONTACT, PRODUCT, CUSTOM }

    private enum class NodeKind { STRING, BOX, COLUMN, ROW, SPACER, TEXT, BUTTON, TEXT_FIELD, IMAGE, EFFECT }

    private fun kind(body: ChatMessageBody) = when (body) {
        is ChatMessageBody.Text -> BodyKind.TEXT
        is ChatMessageBody.RichText -> if (body.media.isEmpty()) BodyKind.MARKDOWN else BodyKind.RICH_TEXT_WITH_MEDIA
        is ChatMessageBody.Actions -> if (body.v1.layout == ChatActionLayout.GRID) BodyKind.ACTIONS_GRID else BodyKind.ACTIONS_COLUMN
        is ChatMessageBody.Custom -> BodyKind.CUSTOM
        is ChatMessageBody.Notice -> BodyKind.NOTICE
    }

    private fun kind(item: ChatMediaItem) = when (item) {
        is ChatMediaItem.Image -> MediaKind.IMAGE
        is ChatMediaItem.Video -> MediaKind.VIDEO
        is ChatMediaItem.File -> MediaKind.FILE
    }

    private fun kind(participant: ChatParticipant) = when (participant) {
        is ChatParticipant.Person -> ParticipantKind.PERSON
        is ChatParticipant.LitePerson -> ParticipantKind.LITE_PERSON
        is ChatParticipant.Contact -> ParticipantKind.CONTACT
        is ChatParticipant.Product -> ParticipantKind.PRODUCT
        is ChatParticipant.Custom -> ParticipantKind.CUSTOM
    }

    // What a set of trees uses: node kinds, typography, colours of text and backgrounds, and button variants.
    private class TreeUsage {
        val nodes = mutableSetOf<NodeKind>()
        val typography = mutableSetOf<TypographyStyle>()
        val colours = mutableSetOf<ColorToken>()
        val buttons = mutableSetOf<ButtonVariant>()

        fun visit(node: RendererNode) {
            when (node) {
                RendererNode.Nil -> Unit
                is RendererNode.String -> nodes += NodeKind.STRING
                is RendererNode.Box -> visit(NodeKind.BOX, node.modifiers, node.children)
                is RendererNode.Column -> visit(NodeKind.COLUMN, node.modifiers, node.children)
                is RendererNode.Row -> visit(NodeKind.ROW, node.modifiers, node.children)
                is RendererNode.Spacer -> visit(NodeKind.SPACER, node.modifiers, emptyList())
                is RendererNode.Text -> {
                    node.props.style?.let { typography += it }
                    node.props.color?.let { colours += it }
                    visit(NodeKind.TEXT, node.modifiers, node.children)
                }
                is RendererNode.Button -> {
                    node.props.variant?.let { buttons += it }
                    visit(NodeKind.BUTTON, node.modifiers, node.children)
                }
                is RendererNode.TextField -> visit(NodeKind.TEXT_FIELD, node.modifiers, emptyList())
                is RendererNode.Image -> visit(NodeKind.IMAGE, node.modifiers, emptyList())
                is RendererNode.Effect -> visit(NodeKind.EFFECT, emptyList(), node.children)
            }
        }

        private fun visit(kind: NodeKind, modifiers: List<Modifier>, children: List<RendererNode>) {
            nodes += kind
            modifiers.filterIsInstance<Modifier.Background>().forEach { colours += it.v1.color }
            children.forEach(::visit)
        }
    }
}
