package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.markdownPlainText
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber

/** What the host draws for one message besides its body (07-LATER §9). */
@Immutable
data class MessageDecorationUiModel(
    val thread: ThreadChipUiModel?,
    val branch: BranchControlUiModel?,
    val reasoning: ReasoningRowUiModel?,
)

/** The chip under the root of a thread. [hasUnread] draws a dot. */
@Immutable
data class ThreadChipUiModel(
    val threadChatId: ChatId,
    val replyCount: Long,
    /** Milliseconds since the Unix epoch. */
    val lastReplyAt: Long?,
    val repliers: ImmutableList<ChatParticipantUiModel>,
    val hasUnread: Boolean,
)

/** "‹ index/count ›" under a message. While [waiting], the product did not answer the last arrow yet. */
@Immutable
data class BranchControlUiModel(val index: Int, val count: Int, val waiting: Boolean)

/** A reasoning row in place of a bubble. While [thinking], the message is `Preparing`. */
@Immutable
data class ReasoningRowUiModel(val thinking: Boolean, val durationMs: Long?)

/** The top of a thread room: the name of the parent room and the pinned root. A null [rootText] is "Earlier message". */
@Immutable
data class ThreadRoomUiState(val parentName: String?, val rootText: String?)

/**
 * The meta of the messages of a product room as the room screen draws it (07-LATER §9): thread chips, branch controls
 * and reasoning rows, each only when the host lists its feature in [servedFeatures]. It also keeps the branch arrows
 * that wait for the product, and the top of a thread room.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomDecorations(
    private val scope: CoroutineScope,
    private val chatId: ChatId,
    roomPolicy: Flow<ChatRoomPolicy?>,
    feedSnapshot: Flow<ChatFeedSnapshot>,
    private val feed: ProductRoomAdditionsFeed,
    private val participants: ProductRoomParticipants,
    private val servedFeatures: Collection<String> = ChatHostCapabilities.features,
) {
    private class PendingBranch(val index: Int, val timer: Job)

    private val pendingBranches = MutableStateFlow<Map<String, PendingBranch>>(emptyMap())

    private val meta: StateFlow<Map<String, ChatMessageMetaModel>> = feed.messageMeta(chatId)
        .onStart { emit(emptyMap()) }
        .stateIn(scope, SharingStarted.Eagerly, emptyMap())

    /** The newest snapshot, for the menu of a message. */
    private val snapshot: StateFlow<ChatFeedSnapshot?> = feedSnapshot.stateIn(scope, SharingStarted.Eagerly, null)

    private val preparing = combine(
        feed.preparingMessages(chatId).onStart { emit(emptySet()) },
        feedSnapshot.map { it.preparingProductMessages }.distinctUntilChanged(),
    ) { hostStore, productSource -> hostStore + productSource }

    private val threadRoomIds = meta
        .map { all -> all.values.mapNotNullTo(mutableSetOf()) { it.thread?.roomId } }
        .distinctUntilChanged()

    private val unreadThreads = threadRoomIds.flatMapLatest { roomIds ->
        val extensionId = chatId.extensionOrNull()?.extensionId
        if (roomIds.isEmpty() || extensionId == null || THREADS !in servedFeatures) {
            flowOf(emptySet())
        } else {
            feed.roomsWithUnread(roomIds.mapTo(mutableSetOf()) { ChatId.forExtensionRoom(extensionId, it) }).onStart { emit(emptySet()) }
        }
    }

    val decorations: StateFlow<ImmutableMap<ChatMessageId, MessageDecorationUiModel>> = combine(
        feedSnapshot,
        meta,
        preparing,
        combine(pendingBranches, unreadThreads, participants.personas) { pending, unread, _ -> pending to unread },
    ) { snapshot, meta, preparing, (pending, unread) ->
        if (meta.isEmpty()) return@combine persistentMapOf()
        snapshot.messages.mapNotNull { message ->
            val productId = ProductMessageIds.product(message.id, chatId)
            val messageMeta = meta[productId] ?: return@mapNotNull null
            decoration(message, productId, messageMeta, preparing, pending, unread)?.let { message.id to it }
        }.toMap().toImmutableMap()
    }.stateIn(scope, SharingStarted.Eagerly, persistentMapOf())

    val threadRoom: StateFlow<ThreadRoomUiState?> = roomPolicy
        .map { policy -> policy?.features?.threadOf?.takeIf { THREADS in servedFeatures } }
        .distinctUntilChanged()
        .mapLatest { parent ->
            parent ?: return@mapLatest null
            val root = feed.threadRoot(chatId, parent)
            val text = root.rootText?.let { if (it.markdown) markdownPlainText(it.text) else it.text }
            ThreadRoomUiState(parentName = root.parentName, rootText = text)
        }
        .stateIn(scope, SharingStarted.Eagerly, null)

    init {
        // The answer of the product moves the position, which ends the wait.
        scope.launch {
            meta.collect { all ->
                pendingBranches.value.forEach { (productId, waiting) ->
                    if (all[productId]?.branch?.index == waiting.index) {
                        waiting.timer.cancel()
                        pendingBranches.update { it - productId }
                    }
                }
            }
        }
    }

    /** A reasoning row offers Copy only (07-LATER §9.4). */
    fun isReasoning(messageId: ChatMessageId): Boolean = decorations.value[messageId]?.reasoning != null

    /**
     * Sends `BranchSelected` and waits for the product to move the position. Without an answer in 5 s the control
     * takes the stored position again (07-LATER §9.3).
     */
    fun selectBranch(messageId: ChatMessageId, index: Int) {
        val productId = ProductMessageIds.product(messageId, chatId)
        val branch = meta.value[productId]?.branch ?: return
        if (index !in 1..branch.count || index == branch.index || productId in pendingBranches.value) return

        val timer = scope.launch {
            delay(BRANCH_ANSWER_TIMEOUT_MS)
            Timber.i("truapi.chat.branch %s: no answer for %d in %d ms", productId, index, BRANCH_ANSWER_TIMEOUT_MS)
            pendingBranches.update { it - productId }
        }
        pendingBranches.update { it + (productId to PendingBranch(index, timer)) }
        scope.launch { feed.publish(chatId, ChatRoomEventModel.BranchSelected(productId, index)) }
    }

    /** The plain text that "Read aloud" speaks for a message: `Text`, and `RichText` without its Markdown marks. */
    fun speakableText(messageId: ChatMessageId): String? {
        val message = snapshot.value?.messages?.find { it.id == messageId } ?: return null
        val text = when (val content = message.content) {
            is ChatMessage.Content.Text -> content.text
            is ChatMessage.Content.RichText -> content.text?.let(::markdownPlainText)
            is ChatMessage.Content.ProductBody -> (content.body as? ProductMessageBody.RichText)?.text?.let(::markdownPlainText)
            else -> null
        }
        return text?.takeIf { it.isNotBlank() }
    }

    @Suppress("LongParameterList")
    private suspend fun decoration(
        message: ChatMessage,
        productId: String,
        meta: ChatMessageMetaModel,
        preparing: Set<String>,
        pending: Map<String, PendingBranch>,
        unreadThreads: Set<ChatId>,
    ): MessageDecorationUiModel? {
        if (message.content is ChatMessage.Content.Deleted) return null

        val waiting = pending[productId]
        val decoration = MessageDecorationUiModel(
            thread = meta.thread?.takeIf { THREADS in servedFeatures }?.let { thread ->
                val extensionId = chatId.extensionOrNull()?.extensionId ?: return@let null
                val threadChatId = ChatId.forExtensionRoom(extensionId, thread.roomId)
                val repliers = participants.appearancesOf(thread.participantIds)
                ThreadChipUiModel(
                    threadChatId = threadChatId,
                    replyCount = thread.replyCount,
                    lastReplyAt = thread.lastReplyAt,
                    repliers = thread.participantIds.mapNotNull { repliers[it] }.toImmutableList(),
                    hasUnread = threadChatId in unreadThreads,
                )
            },
            branch = meta.branch?.takeIf { BRANCHES in servedFeatures }?.let {
                BranchControlUiModel(index = it.index, count = it.count, waiting = waiting != null && it.index != waiting.index)
            },
            reasoning = meta.reasoning?.takeIf { REASONING_ROWS in servedFeatures }?.let {
                ReasoningRowUiModel(
                    thinking = productId in preparing,
                    durationMs = it.durationMs,
                )
            },
        )
        return decoration.takeIf { it.thread != null || it.branch != null || it.reasoning != null }
    }

    private companion object {
        const val BRANCH_ANSWER_TIMEOUT_MS = 5_000L
        val THREADS = ChatHostCapabilities.THREADS
        val BRANCHES = ChatHostCapabilities.BRANCHES
        val REASONING_ROWS = ChatHostCapabilities.REASONING_ROWS
    }
}
