package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Participant
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Room
import uniffi.truapi.ChatAction
import uniffi.truapi.ChatActionLayout
import uniffi.truapi.ChatActions
import uniffi.truapi.ChatBranchPosition
import uniffi.truapi.ChatMediaItem
import uniffi.truapi.ChatMediaMeta
import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMeta
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ChatReasoning
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatThreadSummary

/** The messages of the mock product, oldest first, with their timestamps before `now`. */
object MockChatShowcaseMessages {
    object CustomType {
        const val CARD = "showcase.card"
        const val COUNTER = "showcase.counter"
    }

    val branchTexts = listOf(
        "Branch 1 of 3: the first alternative of this answer. Use the arrows under it.",
        "Branch 2 of 3: the product swapped the body with an `Edited` update.",
        "Branch 3 of 3: the last alternative. The position moves with `set_message_meta`.",
    )
    const val BRANCH_MESSAGE_ID = "m-branch"
    const val REASONING_MESSAGE_ID = "m-reasoning"

    /** Older messages, so the room screen asks for a second page when the user scrolls up. */
    const val HISTORY_COUNT = 60

    private val minute: ULong = 60_000u
    private val day: ULong = minute * 1_440u

    fun showcase(media: MockChatMedia, now: ULong): List<ChatMessage> {
        val history = (1..HISTORY_COUNT).map { number ->
            val author = if (number % 3 == 0) Participant.USER else Participant.BOB
            message("m-history-$number", author, ChatMessageBody.Text("Earlier message $number of $HISTORY_COUNT, for paging."))
        }
        val showcase = bodies(media) + authors() + statuses() + extras()
        return timed(history, endingAt = now - day * 2u, spacing = minute * 5u) + timed(showcase, endingAt = now - minute, spacing = minute * 3u)
    }

    /** The meta that the mock product sets on `showcase`. */
    fun showcaseMeta(now: ULong): List<Pair<String, ChatMessageMeta>> = listOf(
        MockChatShowcase.THREAD_ROOT_ID to ChatMessageMeta(
            thread = ChatThreadSummary(
                roomId = Room.THREAD,
                replyCount = thread(now).size.toUInt(),
                lastReplyAt = thread(now).last().timestamp,
                participantIds = listOf(Participant.ALICE, Participant.BOB, Participant.ASSISTANT),
            ),
            branch = null,
            reasoning = null,
        ),
        BRANCH_MESSAGE_ID to ChatMessageMeta(thread = null, branch = ChatBranchPosition(1u, branchTexts.size.toUShort()), reasoning = null),
        REASONING_MESSAGE_ID to ChatMessageMeta(thread = null, branch = null, reasoning = ChatReasoning(4_200u)),
    )

    /** The replies of the thread room, which the host stores. A product writes no message of the user there. */
    fun thread(now: ULong): List<ChatMessage> = timed(
        listOf(
            message("t-1", Participant.ALICE, ChatMessageBody.Text("A reply in the thread, stored by the host.")),
            message("t-2", Participant.BOB, ChatMessageBody.RichText("Threads are rooms: **every** feature works.", emptyList())),
            message("t-3", Participant.ASSISTANT, ChatMessageBody.Text("Reply here, and the summary under the root updates.")),
        ),
        endingAt = now - minute * 4u,
        spacing = minute,
    )

    /** The messages of `store`: the bodies that a product writes with `update_feed`. */
    fun store(media: MockChatMedia, now: ULong): List<ChatMessage> = timed(
        listOf(
            message("s-notice", Participant.PRODUCT, ChatMessageBody.Notice("This room is stored by the host.")),
            message("s-text", Participant.PRODUCT, ChatMessageBody.Text("A Text body, stored by the host.")),
            message(
                "s-markdown",
                Participant.ASSISTANT,
                ChatMessageBody.RichText("A **RichText** body from a persona, with `code` and a [link](https://polkadot.com).", emptyList()),
            ),
            message("s-image", Participant.PRODUCT, ChatMessageBody.RichText("An inline image, stored as a product body.", listOf(image(media.pictures[3], "A green gradient")))),
            message(
                "s-actions",
                Participant.PRODUCT,
                ChatMessageBody.Actions(ChatActions("Buttons of a stored message:", listOf(ChatAction("store-a", "First"), ChatAction("store-b", "Second")), ChatActionLayout.GRID)),
            ),
            message("s-custom", Participant.PRODUCT, ChatMessageBody.Custom(CustomType.COUNTER, ByteArray(0), "A counter drawn by the product")),
        ),
        endingAt = now - minute * 10u,
        spacing = minute,
    )

    fun message(messageId: String, participantId: String, body: ChatMessageBody, status: ChatMessageStatus = ChatMessageStatus.SENT) = ChatMessage(
        messageId = messageId,
        origin = ChatMessageOrigin(participantId),
        timestamp = 0u,
        body = body,
        status = status,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
    )

    private fun bodies(media: MockChatMedia): List<ChatMessage> {
        val video = media.video?.let { bytes ->
            val item = ChatMediaItem.Video(
                ChatResource.Inline("video/mp4", bytes),
                ChatMediaMeta("video/mp4", bytes.size.toULong(), MockChatMedia.VIDEO_SIDE.toUInt(), MockChatMedia.VIDEO_SIDE.toUInt(), MockChatMedia.PLACEHOLDER, "A circle that moves over a colour that changes"),
            )
            message("m-video", Participant.PRODUCT, ChatMessageBody.RichText("RichText with a video, played by the host.", listOf(item)))
        }
        val file = ChatMediaItem.File(
            ChatResource.Inline("application/pdf", media.document),
            ChatMediaMeta("application/pdf", media.document.size.toULong(), width = null, height = null, placeholder = null, alt = "The showcase PDF"),
            fileName = "Chat Showcase.pdf",
        )
        return listOfNotNull(
            message("m-notice", Participant.PRODUCT, ChatMessageBody.Notice("A Notice body: a system row in the middle of the room.")),
            message("m-text", Participant.PRODUCT, ChatMessageBody.Text("A Text body is plain. A link such as https://polkadot.com opens after a confirmation.")),
            message(
                "m-markdown",
                Participant.PRODUCT,
                ChatMessageBody.RichText(
                    "A **RichText** body without media is Markdown: *italic*, `code`, ~~struck~~ and [a link](https://parity.io).\n- a list item\n- another item",
                    emptyList(),
                ),
            ),
            message("m-image", Participant.PRODUCT, ChatMessageBody.RichText("RichText with one image under the text.", listOf(image(media.pictures[0], "A pink gradient")))),
            message("m-gallery", Participant.PRODUCT, ChatMessageBody.RichText(null, media.pictures.take(3).mapIndexed { index, bytes -> image(bytes, "Gallery image ${index + 1}") })),
            video,
            message("m-file", Participant.PRODUCT, ChatMessageBody.RichText(null, listOf(file))),
            message(
                "m-actions-column",
                Participant.PRODUCT,
                ChatMessageBody.Actions(
                    ChatActions(
                        "An Actions body in a column. Pick one:",
                        listOf(ChatAction("pick-red", "Red"), ChatAction("pick-green", "Green"), ChatAction("pick-blue", "Blue")),
                        ChatActionLayout.COLUMN,
                    ),
                ),
            ),
            message(
                "m-actions-grid",
                Participant.PRODUCT,
                ChatMessageBody.Actions(ChatActions(null, (1..4).map { ChatAction("grid-$it", "Option $it") }, ChatActionLayout.GRID)),
            ),
            message("m-card", Participant.PRODUCT, ChatMessageBody.Custom(CustomType.CARD, "card".encodeToByteArray(), "A custom body drawn by the product")),
            message("m-counter", Participant.PRODUCT, ChatMessageBody.Custom(CustomType.COUNTER, ByteArray(0), "A counter drawn by the product")),
        )
    }

    private fun authors() = listOf(
        message("m-persona", Participant.ASSISTANT, ChatMessageBody.Text("A persona that the product registered with register_bot, drawn with its name and icon.")),
        message("m-persona-support", Participant.SUPPORT, ChatMessageBody.Text("Another persona, Support.")),
        message("m-alice", Participant.ALICE, ChatMessageBody.Text("A Custom participant with an avatar.")).copy(
            reactions = listOf(ChatReactionSummary("👍", 3u, reactedByUser = true), ChatReactionSummary("🎉", 1u, reactedByUser = false)),
        ),
        message("m-bob", Participant.BOB, ChatMessageBody.Text("A Custom participant without an avatar. Edited.")).copy(editedAt = 0u),
        message("m-person", Participant.PERSON, ChatMessageBody.Text("A Person of the People chain, person id 1.")),
        message("m-lite", Participant.LITE_PERSON, ChatMessageBody.Text("A LitePerson, drawn from its account.")),
        message("m-contact", Participant.CONTACT, ChatMessageBody.Text("A Contact handle that this device does not know.")),
        message("m-product", Participant.OTHER_PRODUCT, ChatMessageBody.Text("Another product, drawn from its manifest.")),
        message("m-unknown", "nobody", ChatMessageBody.Text("A participant id that the product does not resolve.")),
    )

    private fun statuses() = listOf(
        message("m-user-read", Participant.USER, ChatMessageBody.Text("My message, read."), ChatMessageStatus.READ),
        message("m-reply", Participant.PRODUCT, ChatMessageBody.Text("A reply to your message, with a quote.")).copy(replyTo = "m-user-read"),
        message("m-user-reply", Participant.USER, ChatMessageBody.Text("My reply to the Markdown message."), ChatMessageStatus.DELIVERED).copy(
            replyTo = "m-markdown",
            reactions = listOf(ChatReactionSummary("❤️", 2u, reactedByUser = false)),
        ),
        message("m-user-sent", Participant.USER, ChatMessageBody.Text("My message, sent.")),
        message("m-user-preparing", Participant.USER, ChatMessageBody.Text("My message, still preparing."), ChatMessageStatus.PREPARING),
        message("m-user-failed", Participant.USER, ChatMessageBody.Text("My message that failed. Retry it."), ChatMessageStatus.FAILED),
    )

    private fun extras() = listOf(
        message(REASONING_MESSAGE_ID, Participant.ASSISTANT, ChatMessageBody.Text("A reasoning row: the product thought about which parts of a room to show first.")),
        message(BRANCH_MESSAGE_ID, Participant.ASSISTANT, ChatMessageBody.Text(branchTexts[0])),
        message(MockChatShowcase.THREAD_ROOT_ID, Participant.ALICE, ChatMessageBody.Text("This message has a thread. Tap the summary under it to open the thread room.")),
        message(
            "m-end",
            Participant.PRODUCT,
            ChatMessageBody.RichText(
                "**Try the rest:** the toolbar, the quick replies, the `/` commands, a long press on a message, the participants, search and the call buttons.",
                emptyList(),
            ),
        ),
    )

    private fun timed(messages: List<ChatMessage>, endingAt: ULong, spacing: ULong): List<ChatMessage> =
        messages.mapIndexed { index, message ->
            val time = endingAt - (messages.size - 1 - index).toULong() * spacing
            message.copy(timestamp = time, editedAt = message.editedAt?.let { time + minute })
        }

    private fun image(bytes: ByteArray, alt: String) = ChatMediaItem.Image(
        ChatResource.Inline("image/jpeg", bytes),
        ChatMediaMeta("image/jpeg", bytes.size.toULong(), MockChatMedia.PICTURE_WIDTH.toUInt(), MockChatMedia.PICTURE_HEIGHT.toUInt(), MockChatMedia.PLACEHOLDER, alt),
    )
}
