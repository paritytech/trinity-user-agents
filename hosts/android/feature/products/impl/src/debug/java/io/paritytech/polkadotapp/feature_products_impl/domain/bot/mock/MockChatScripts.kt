package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Participant
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Room
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCoreBody
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.ChatActivity
import uniffi.truapi.ChatBranchPosition
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMeta
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatReasoning
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatThreadSummary
import uniffi.truapi.ChatUserAction
import uniffi.truapi.NativeChatFeedRejection
import uniffi.truapi.ProductChatUserActionItem
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

/** What the mock product does: the rooms it declares, and how it answers what the user does in them. */
class MockChatScripts(
    val state: MockChatState,
    private val host: MockChatHost,
    private val scope: CoroutineScope,
    private val bodies: ConcurrentHashMap<String, MutableStateFlow<MockChatBody>>,
) {
    val menus = MockChatMenus(state.media)

    private val nextNumber = AtomicInteger()

    suspend fun seed() {
        val now = now()
        val media = state.media
        host.perform("register its personas") {
            MockChatShowcase.personas(media).forEach { registerBot(it.botId, it.name, it.icon) }
        }
        host.perform("declare the showcase room") {
            declareRoom(Room.SHOWCASE, MockChatShowcase.showcaseMetadata(media), MockChatShowcase.showcaseConfig, MockChatShowcase.Preview.PINNED.wireValue(now, media))
            setRoomFeatures(Room.SHOWCASE, MockChatShowcase.showcaseFeatures)
            setRoomToolbar(Room.SHOWCASE, MockChatShowcase.toolbar(media))
            setRoomSuggestions(Room.SHOWCASE, MockChatShowcase.suggestions)
            setRoomCommands(Room.SHOWCASE, MockChatShowcase.commands)
            setRoomFooter(Room.SHOWCASE, state.footer.wireValue)
            MockChatShowcaseMessages.showcaseMeta(now).forEach { (messageId, meta) -> setMessageMeta(Room.SHOWCASE, messageId, meta) }
        }
        host.perform("declare the thread room") {
            declareRoom(Room.THREAD, MockChatShowcase.threadMetadata, MockChatShowcase.threadConfig, preview = null)
            setRoomFeatures(Room.THREAD, MockChatShowcase.threadFeatures)
            setRoomFooter(Room.THREAD, MockChatShowcase.threadFooter)
            storeOnce(Room.THREAD, MockChatShowcaseMessages.thread(now))
        }
        host.perform("declare the stored room") {
            declareRoom(Room.STORE, MockChatShowcase.storeMetadata(media), MockChatShowcase.storeConfig, preview = null)
            storeOnce(Room.STORE, MockChatShowcaseMessages.store(media, now))
        }
        Timber.d("Chat showcase declared its rooms")
    }

    /** "reject" is refused, "fail" is created as failed, and every other message of the user gets an answer. */
    suspend fun userAction(action: ChatUserAction): Result<ProductChatUserActionItem> {
        val found = when (action) {
            is ChatUserAction.NewMessage -> return post(action.body, action.replyTo)
            is ChatUserAction.EditMessage -> state.read { it.edit(action.messageId, action.body, now()) }
            is ChatUserAction.DeleteMessage -> state.read { it.remove(action.messageId) }
            is ChatUserAction.ReactionPlaced -> state.read { it.react(action.messageId, action.emoji, placed = true, byUser = true) }
            is ChatUserAction.ReactionRemoved -> state.read { it.react(action.messageId, action.emoji, placed = false, byUser = true) }
            is ChatUserAction.Seen -> true
            is ChatUserAction.Retry -> state.read { it.message(action.messageId) != null }.also { found ->
                if (found) launchScript { deliver(ChatFeedUpdate.StatusChanged(action.messageId, ChatMessageStatus.SENT)) }
            }
        }
        return if (found) Result.success(ProductChatUserActionItem.Acknowledged) else Result.failure(ProductChatQueryException.UnknownMessage())
    }

    suspend fun onEvent(roomId: String, event: ProductRoomEvent) {
        when (event) {
            is ProductRoomEvent.MessagePosted -> event.content.toCoreBody()?.searchText()?.let { launchAnswerStored(it, roomId) }
            is ProductRoomEvent.ActionTriggered -> launchScript {
                if (roomId == Room.SHOWCASE) say("You pressed “${event.actionId}”.", replyTo = event.messageId) else sayStored("You pressed “${event.actionId}”.", roomId)
            }
            is ProductRoomEvent.Command -> MockChatShowcase.Command.named(event.name)?.let { command -> launchScript { execute(command, event.arguments.trim()) } }
            is ProductRoomEvent.MessageActionTriggered -> launchScript { pickMessageAction(event.actionId, event.messageId, roomId) }
            is ProductRoomEvent.ParticipantActionTriggered -> launchScript { notice("You picked “${event.actionId}” for the participant ${event.participantId}.") }
            is ProductRoomEvent.ToolbarActionTriggered -> launchScript { press(event.actionId) }
            is ProductRoomEvent.SuggestionSelected -> launchScript { press(event.suggestionId) }
            is ProductRoomEvent.UserEdited,
            is ProductRoomEvent.UserDeleted,
            is ProductRoomEvent.UserReacted,
            is ProductRoomEvent.Seen,
            ProductRoomEvent.RoomOpened,
            ProductRoomEvent.RoomClosed,
            -> Unit
        }
    }

    fun onRoomEvent(roomId: String, event: ChatRoomEventModel) {
        if (roomId != Room.SHOWCASE) return
        when (event) {
            is ChatRoomEventModel.CallRequested -> launchScript { startCall(event.video) }
            is ChatRoomEventModel.CallAnswered -> launchScript { if (event.accept) startCall(video = false) else endCall("You declined the call.") }
            is ChatRoomEventModel.CallOpened -> launchScript { notice("A product without a media session opens its own call page here.") }
            is ChatRoomEventModel.CallEnded -> launchScript { endCall("You ended the call.") }
            is ChatRoomEventModel.BranchSelected -> launchScript { selectBranch(event.messageId, event.index) }
            is ChatRoomEventModel.UserStateChanged -> Timber.d("Chat showcase: muted until %s, archived %s", event.state.mutedUntil, event.state.archived)
        }
    }

    fun onFooterAction(roomId: String, actionId: String) {
        if (roomId != Room.SHOWCASE) return
        when (actionId) {
            MockChatRenderTrees.Action.FOOTER_YES, MockChatRenderTrees.Action.FOOTER_NO -> launchScript {
                val text = if (actionId == MockChatRenderTrees.Action.FOOTER_YES) "👍" else "👎"
                val posted = message("footer", Participant.USER, ChatMessageBody.Text(text))
                deliver(ChatFeedUpdate.New(posted))
                answer(posted)
            }
            MockChatRenderTrees.Action.FOOTER_INPUT -> launchScript { setFooter(MockChatShowcase.Footer.INPUT) }
        }
    }

    suspend fun closeRoom(roomId: String) {
        host.perform("clear the activity of $roomId") { setRoomActivity(roomId, activity = null) }
    }

    /** A message of the user in a room that the host stores. In the thread, the summary under the root counts it. */
    fun launchAnswerStored(text: String, roomId: String) = launchScript {
        delay(600)
        sayStored("Stored: “$text”.", roomId, Participant.ASSISTANT)
        if (roomId != Room.THREAD) return@launchScript

        state.threadReplies += 2
        val summary = ChatThreadSummary(Room.THREAD, state.threadReplies.toUInt(), now(), listOf(Participant.ASSISTANT, Participant.ALICE, Participant.BOB))
        host.perform("update the thread summary") {
            setMessageMeta(Room.SHOWCASE, MockChatShowcase.THREAD_ROOT_ID, ChatMessageMeta(thread = summary, branch = null, reasoning = null))
        }
    }

    private suspend fun post(body: ChatMessageBody, replyTo: String?): Result<ProductChatUserActionItem> {
        val text = body.searchText()?.trim()?.lowercase()
        if (text == "reject") return Result.failure(ProductChatQueryException.Rejected("The showcase refuses a message that says “reject”."))

        val status = if (text == "fail") ChatMessageStatus.FAILED else ChatMessageStatus.SENT
        val created = message("user", Participant.USER, body, status).copy(replyTo = replyTo)
        state.read { it.append(created) }
        if (status == ChatMessageStatus.SENT) launchScript { answer(created) }
        return Result.success(ProductChatUserActionItem.MessageCreated(created))
    }

    // The product marks the message delivered, types, answers, and marks the message read.
    private suspend fun answer(userMessage: ChatMessage) {
        if (userMessage.body.searchText() == MockChatShowcase.suggestions.first().text) return stream()

        delay(400)
        deliver(ChatFeedUpdate.StatusChanged(userMessage.messageId, ChatMessageStatus.DELIVERED))
        setActivity("Assistant is typing…")
        delay(1_200)
        val reply = if (userMessage.body.hasMedia()) "Thanks for the attachment." else "You said: “${userMessage.body.searchText().orEmpty()}”. Try “reject” or “fail” too."
        say(reply, Participant.ASSISTANT, replyTo = userMessage.messageId)
        setActivity(null)
        deliver(ChatFeedUpdate.StatusChanged(userMessage.messageId, ChatMessageStatus.READ))
    }

    private suspend fun execute(command: MockChatShowcase.Command, argument: String) {
        when (command) {
            MockChatShowcase.Command.STREAM -> stream()
            MockChatShowcase.Command.TYPING -> {
                setActivity("Alice is typing…", Participant.ALICE)
                delay(5_000)
                setActivity(null)
            }
            MockChatShowcase.Command.RING -> ring(video = argument == "video")
            MockChatShowcase.Command.CALL -> startCall(video = argument == "video")
            MockChatShowcase.Command.HANGUP -> endCall("You hung up with /hangup.")
            MockChatShowcase.Command.FOOTER -> setFooter(MockChatShowcase.Footer.named(argument) ?: state.footer.next)
            MockChatShowcase.Command.PREVIEW -> setPreview(MockChatShowcase.Preview.named(argument) ?: MockChatShowcase.Preview.DEFAULT)
            MockChatShowcase.Command.NOTICE -> notice(argument.ifEmpty { "A notice that /notice posted." })
            MockChatShowcase.Command.BURST -> repeat(20) { index ->
                say("Burst message ${index + 1} of 20", if (index % 2 == 0) Participant.BOB else Participant.ALICE)
            }
        }
    }

    private suspend fun press(actionId: String) {
        when (actionId) {
            "ring" -> ring(video = false)
            "stream" -> stream()
            "footer" -> setFooter(state.footer.next)
            else -> notice("The showcase got “$actionId”.")
        }
    }

    private suspend fun pickMessageAction(actionId: String, messageId: String, roomId: String) {
        if (roomId != Room.SHOWCASE) return sayStored("You picked “$actionId” on a stored message.", roomId)

        when (actionId) {
            MockChatMenus.Action.HIDE -> deliver(ChatFeedUpdate.Deleted(messageId))
            MockChatMenus.Action.REACT -> deliver(ChatFeedUpdate.ReactionPlaced(messageId, Participant.ALICE, "🎉"))
            MockChatMenus.Action.REFRESH -> {
                bodies[messageId]?.let { it.value = it.value.initial() }
                notice("The product drew the body again.")
            }
            else -> notice("You picked “$actionId” on a message.")
        }
    }

    // A reasoning row that streams and collapses, then an answer that streams (07-LATER §9.4).
    private suspend fun stream() {
        val started = now()
        setActivity("Assistant is thinking…")
        val reasoning = message("reasoning", Participant.ASSISTANT, ChatMessageBody.Text(""), ChatMessageStatus.PREPARING)
        deliver(ChatFeedUpdate.New(reasoning))
        setMeta(reasoning.messageId, ChatMessageMeta(thread = null, branch = null, reasoning = ChatReasoning(durationMs = null)))
        streamText("The user asked for a stream. I edit this row while I think, then I mark it sent.", reasoning.messageId)
        deliver(ChatFeedUpdate.StatusChanged(reasoning.messageId, ChatMessageStatus.SENT))
        setMeta(reasoning.messageId, ChatMessageMeta(thread = null, branch = null, reasoning = ChatReasoning((now() - started).toUInt())))

        setActivity("Assistant is typing…")
        val answer = message("answer", Participant.ASSISTANT, ChatMessageBody.Text(""), ChatMessageStatus.PREPARING)
        deliver(ChatFeedUpdate.New(answer))
        streamText("A product streams a long answer as `New` with the status Preparing, then `Edited` as text arrives, then `StatusChanged` to Sent.", answer.messageId)
        deliver(ChatFeedUpdate.StatusChanged(answer.messageId, ChatMessageStatus.SENT))
        setActivity(null)
    }

    private suspend fun streamText(text: String, messageId: String) {
        val words = text.split(" ")
        for (count in 1..words.size) {
            deliver(ChatFeedUpdate.Edited(messageId, ChatMessageBody.Text(words.take(count).joinToString(" ")), now()))
            delay(90)
        }
    }

    private suspend fun ring(video: Boolean) {
        delay(1_000)
        host.perform("ring") { setRoomCall(Room.SHOWCASE, ChatRoomCall.Ringing(nextId("call"), video, caller = Participant.ALICE)) }
    }

    private suspend fun startCall(video: Boolean) {
        host.perform("start a call") { setRoomCall(Room.SHOWCASE, ChatRoomCall.Active(nextId("call"), video, now(), mediaSession = null)) }
        notice(if (video) "A video call is active. Tap the bar or End." else "A call is active. Tap the bar or End.")
    }

    private suspend fun endCall(reason: String) {
        host.perform("end the call") { setRoomCall(Room.SHOWCASE, call = null) }
        notice(reason)
    }

    private suspend fun selectBranch(messageId: String, index: Int) {
        val texts = MockChatShowcaseMessages.branchTexts
        if (messageId != MockChatShowcaseMessages.BRANCH_MESSAGE_ID || index !in 1..texts.size) return

        deliver(ChatFeedUpdate.Edited(messageId, ChatMessageBody.Text(texts[index - 1]), now()))
        setMeta(messageId, ChatMessageMeta(thread = null, branch = ChatBranchPosition(index.toUShort(), texts.size.toUShort()), reasoning = null))
    }

    private suspend fun setFooter(footer: MockChatShowcase.Footer) {
        state.footer = footer
        host.perform("set the footer") { setRoomFooter(Room.SHOWCASE, footer.wireValue) }
    }

    private suspend fun setPreview(preview: MockChatShowcase.Preview) {
        host.perform("set the preview") { setRoomPreview(Room.SHOWCASE, preview.wireValue(now(), state.media)) }
        notice("The chat list shows the preview “${preview.wireName}”.")
    }

    private suspend fun deliver(update: ChatFeedUpdate) {
        state.apply(update)
        host.perform("push an update to the showcase") { updateFeed(Room.SHOWCASE, update, persist = false) }
    }

    private suspend fun notice(text: String) = deliver(ChatFeedUpdate.New(message("notice", Participant.PRODUCT, ChatMessageBody.Notice(text))))

    private suspend fun say(text: String, participantId: String = Participant.PRODUCT, replyTo: String? = null) =
        deliver(ChatFeedUpdate.New(message("say", participantId, ChatMessageBody.Text(text)).copy(replyTo = replyTo)))

    private suspend fun sayStored(text: String, roomId: String, participantId: String = Participant.PRODUCT) {
        val update = ChatFeedUpdate.New(message("store", participantId, ChatMessageBody.Text(text)))
        host.perform("write to $roomId") { updateFeed(roomId, update, persist = true) }
    }

    private suspend fun setActivity(text: String?, participantId: String = Participant.ASSISTANT) {
        host.perform("set the activity line") { setRoomActivity(Room.SHOWCASE, text?.let { ChatActivity(it, participantId) }) }
    }

    private suspend fun setMeta(messageId: String, meta: ChatMessageMeta) {
        host.perform("set the meta of $messageId") { setMessageMeta(Room.SHOWCASE, messageId, meta) }
    }

    private fun message(prefix: String, participantId: String, body: ChatMessageBody, status: ChatMessageStatus = ChatMessageStatus.SENT) =
        MockChatShowcaseMessages.message(nextId(prefix), participantId, body, status).copy(timestamp = now())

    private fun nextId(prefix: String) = "$prefix-${now()}-${nextNumber.incrementAndGet()}"

    private fun launchScript(script: suspend () -> Unit) = scope.launch { script() }

    // `create_room` of the core: the room, then what a new room declares. The core gives an inline avatar as the icon.
    private suspend fun ProductChatHostBridge.declareRoom(roomId: String, metadata: ChatRoomMetadata, config: ChatRoomConfig, preview: ChatRoomPreview?) {
        val icon = (metadata.avatar as? ChatResource.Inline)?.let { "data:${it.mimeType};base64," + Base64.getEncoder().encodeToString(it.bytes) }.orEmpty()
        createRoom(roomId, metadata.name, icon)
        setRoomMetadata(roomId, metadata)
        setRoomConfig(roomId, config)
        preview?.let { setRoomPreview(roomId, it) }
    }

    // The host keeps a stored room across launches, so a message that it holds already is refused as a duplicate.
    private suspend fun ProductChatHostBridge.storeOnce(roomId: String, messages: List<ChatMessage>) {
        messages.forEach { message ->
            try {
                updateFeed(roomId, ChatFeedUpdate.New(message), persist = true)
            } catch (duplicate: NativeChatFeedRejection.DuplicateMessage) {
                Timber.d("Chat showcase keeps %s in %s", message.messageId, roomId)
            }
        }
    }
}
