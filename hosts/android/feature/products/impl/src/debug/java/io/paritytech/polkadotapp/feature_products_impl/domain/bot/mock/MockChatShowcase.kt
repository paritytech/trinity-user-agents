package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatMedia.Glyph
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatCommandSpec
import uniffi.truapi.ChatInputConfig
import uniffi.truapi.ChatInputPayment
import uniffi.truapi.ChatParticipant
import uniffi.truapi.ChatParticipantEntry
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomCalls
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomOrder
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomPreviewText
import uniffi.truapi.ChatSuggestion
import uniffi.truapi.ChatThreadParent
import uniffi.truapi.ChatToolbarAction
import uniffi.truapi.ContactHandle
import java.util.Base64

/**
 * What the mock product declares, in the wire values of Chat Extension v2: three rooms, their participants and
 * personas, and every declaration that a product can set.
 *
 * - `showcase` keeps its messages in the product and declares every part of a room.
 * - `showcase-thread` is the thread of one message of `showcase`, stored by the host.
 * - `store` is stored by the host and shows the same bodies through the host store.
 */
object MockChatShowcase {
    object Room {
        const val SHOWCASE = "showcase"
        const val THREAD = "showcase-thread"
        const val STORE = "store"
    }

    object Participant {
        const val USER = "user"
        const val PRODUCT = "product"
        const val ASSISTANT = "bot:assistant"
        const val SUPPORT = "bot:support"
        const val ALICE = "alice"
        const val BOB = "bob"
        const val PERSON = "person-1"
        const val LITE_PERSON = "lite-1"
        const val CONTACT = "contact-1"
        const val OTHER_PRODUCT = "product-1"

        fun guest(number: Int) = "guest-%02d".format(number)
    }

    val reactions = listOf("👍", "❤️", "😂", "🎉", "🤔", "👀")
    const val GUEST_COUNT = 30
    const val THREAD_ROOT_ID = "m-thread-root"

    fun showcaseMetadata(media: MockChatMedia) = ChatRoomMetadata(
        name = "Chat Showcase",
        avatar = png(media.avatar(Glyph.STACK, INDIGO)),
        description = "Every part of a product room, drawn by the host from the answers of a mock product.",
    )

    val showcaseConfig = ChatRoomConfig(
        productSource = true,
        reactionDetails = true,
        participants = true,
        messageActions = true,
        participantActions = true,
        allowedReactions = reactions,
        placeholder = "No messages yet. Say hi!",
        reportSeen = true,
    )

    val showcaseFeatures = ChatRoomFeatures(
        preimageAttachments = false,
        search = true,
        calls = ChatRoomCalls(audio = true, video = true),
        threadOf = null,
        readAloud = true,
        sealedAttachments = false,
        inputEvents = true,
    )

    val threadMetadata = ChatRoomMetadata(name = "Thread", avatar = null, description = "The thread of a message of Chat Showcase, stored by the host.")

    val threadConfig = ChatRoomConfig(
        productSource = false,
        reactionDetails = false,
        participants = false,
        messageActions = false,
        participantActions = false,
        allowedReactions = null,
        placeholder = "No replies yet",
        reportSeen = false,
    )

    val threadFeatures = ChatRoomFeatures(
        preimageAttachments = false,
        search = false,
        calls = null,
        threadOf = ChatThreadParent(roomId = Room.SHOWCASE, messageId = THREAD_ROOT_ID),
        readAloud = false,
        sealedAttachments = false,
        inputEvents = false,
    )

    val threadFooter = ChatRoomFooter.Input(ChatInputConfig(placeholder = "Reply in the thread…", photo = false, video = false, file = false, payment = null))

    fun storeMetadata(media: MockChatMedia) = ChatRoomMetadata(
        name = "Showcase (host store)",
        avatar = png(media.avatar(Glyph.TRAY, TEAL)),
        description = "A room whose messages the host stores, with the same bodies as Chat Showcase.",
    )

    val storeConfig = ChatRoomConfig(
        productSource = false,
        reactionDetails = false,
        participants = false,
        messageActions = true,
        participantActions = false,
        allowedReactions = null,
        placeholder = "The host stores this room",
        reportSeen = true,
    )

    fun toolbar(media: MockChatMedia) = listOf(
        ChatToolbarAction("ring", "Ring me", png(media.icon(Glyph.PHONE)), ChatActionHandler.Callback),
        ChatToolbarAction("stream", "Stream an answer", png(media.icon(Glyph.SPARKLES)), ChatActionHandler.Callback),
        ChatToolbarAction("footer", "Next footer", png(media.icon(Glyph.FOOTER)), ChatActionHandler.Callback),
        ChatToolbarAction("docs", "Open polkadot.com", png(media.icon(Glyph.LINK)), ChatActionHandler.Deeplink("https://polkadot.com")),
    )

    val suggestions = listOf(
        ChatSuggestion("stream", "Stream an answer", sendsText = true),
        ChatSuggestion("ring", "Ring me", sendsText = false),
        ChatSuggestion("footer", "Next footer", sendsText = false),
        ChatSuggestion("thanks", "Thanks! 🙏", sendsText = true),
    )

    /** The slash commands of `showcase`. Each one shows a part of a room that the messages alone do not. */
    enum class Command(val description: String, val argumentHint: String?) {
        STREAM("Stream a reasoning row and an answer", null),
        TYPING("Show the activity line for five seconds", null),
        RING("Ring the room as an incoming call", "<audio|video>"),
        CALL("Show the bar of an active call", "<audio|video>"),
        HANGUP("Clear the call of the room", null),
        FOOTER("Replace the footer of the room", Footer.entries.joinToString("|", "<", ">") { it.wireName }),
        PREVIEW("Replace the chat list preview", Preview.entries.joinToString("|", "<", ">") { it.wireName }),
        NOTICE("Post a notice row", "<text>"),
        BURST("Post 20 messages at once", null);

        val wireName = name.lowercase()

        val spec get() = ChatCommandSpec(wireName, description, argumentHint)

        companion object {
            fun named(name: String) = entries.firstOrNull { it.wireName == name }
        }
    }

    val commands get() = Command.entries.map { it.spec }

    /** The footers that the toolbar button and `/footer` go through, in order. */
    enum class Footer {
        INPUT,
        PAYMENT,
        EMPTY,
        CUSTOM;

        val wireName = name.lowercase()

        val wireValue: ChatRoomFooter
            get() = when (this) {
                INPUT -> ChatRoomFooter.Input(ChatInputConfig("Message the showcase…", photo = true, video = true, file = true, payment = null))
                PAYMENT -> ChatRoomFooter.Input(
                    ChatInputConfig("Pay or message the showcase…", photo = true, video = false, file = false, payment = ChatInputPayment.Enabled(ByteArray(32) { 0x42 })),
                )
                EMPTY -> ChatRoomFooter.Empty
                CUSTOM -> ChatRoomFooter.Custom
            }

        val next get() = entries[(ordinal + 1) % entries.size]

        companion object {
            fun named(name: String) = entries.firstOrNull { it.wireName == name }
        }
    }

    /** The chat list previews that `/preview` sets. */
    enum class Preview {
        DEFAULT,
        PINNED,
        BADGE,
        IMAGE,
        ONGOING,
        HIDDEN;

        val wireName = name.lowercase()

        fun wireValue(now: ULong, media: MockChatMedia): ChatRoomPreview = when (this) {
            DEFAULT -> ChatRoomPreview(ChatRoomOrder.TIMESTAMP, timestamp = null, badge = null, text = null)
            PINNED -> ChatRoomPreview(ChatRoomOrder.PIN_TO_TOP, now, ChatRoomBadge.UnreadCount(2u), ChatRoomPreviewText("Pinned: everything a product can draw", ongoing = false))
            BADGE -> ChatRoomPreview(ChatRoomOrder.TIMESTAMP, now, ChatRoomBadge.UnreadCount(42u), text = null)
            IMAGE -> ChatRoomPreview(ChatRoomOrder.TIMESTAMP, now, ChatRoomBadge.Image(png(media.icon(Glyph.STAR))), ChatRoomPreviewText("A badge drawn from an image", ongoing = false))
            ONGOING -> ChatRoomPreview(ChatRoomOrder.TIMESTAMP, now, ChatRoomBadge.Hidden, ChatRoomPreviewText("Assistant is typing", ongoing = true))
            HIDDEN -> ChatRoomPreview(ChatRoomOrder.TIMESTAMP, now, ChatRoomBadge.Hidden, text = null)
        }

        companion object {
            fun named(name: String) = entries.firstOrNull { it.wireName == name }
        }
    }

    class PersonaSpec(val botId: String, val name: String, val icon: String)

    fun personas(media: MockChatMedia) = listOf(
        PersonaSpec("assistant", "Assistant", dataUrl(media.avatar(Glyph.SPARKLES, PURPLE))),
        PersonaSpec("support", "Support", dataUrl(media.avatar(Glyph.HELP, GREEN))),
    )

    /** One participant of each kind, then guests, so the participant list has more than one page. */
    fun participants(media: MockChatMedia): List<ChatParticipantEntry> {
        val named = listOf(
            ChatParticipantEntry(Participant.ASSISTANT, ChatParticipant.Custom("Assistant", png(media.avatar(Glyph.SPARKLES, PURPLE)))),
            ChatParticipantEntry(Participant.SUPPORT, ChatParticipant.Custom("Support", avatar = null)),
            ChatParticipantEntry(Participant.ALICE, ChatParticipant.Custom("Alice", png(media.avatar(Glyph.HEART, PINK)))),
            ChatParticipantEntry(Participant.BOB, ChatParticipant.Custom("Bob", avatar = null)),
            ChatParticipantEntry(Participant.PERSON, ChatParticipant.Person(1u)),
            ChatParticipantEntry(Participant.LITE_PERSON, ChatParticipant.LitePerson(ByteArray(32) { 0x11 })),
            ChatParticipantEntry(Participant.CONTACT, ChatParticipant.Contact(ContactHandle(ByteArray(32) { 0x22 }))),
            ChatParticipantEntry(Participant.OTHER_PRODUCT, ChatParticipant.Product("truapi-playground.dot")),
        )
        val guests = (1..GUEST_COUNT).map { ChatParticipantEntry(Participant.guest(it), ChatParticipant.Custom("Guest $it", avatar = null)) }
        return named + guests
    }

    fun png(bytes: ByteArray): ChatResource = ChatResource.Inline("image/png", bytes)

    private fun dataUrl(bytes: ByteArray) = "data:image/png;base64," + Base64.getEncoder().encodeToString(bytes)

    private const val INDIGO = 0xFF5856D6.toInt()
    private const val TEAL = 0xFF30B0C7.toInt()
    private const val PURPLE = 0xFFAF52DE.toInt()
    private const val PINK = 0xFFFF2D55.toInt()
    private const val GREEN = 0xFF34C759.toInt()
}
