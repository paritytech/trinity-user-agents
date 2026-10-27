package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListener
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import timber.log.Timber
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * The messages of a product-source room: a window of pages that the product gives.
 *
 * The window opens on the newest page and pages older with the cursor of each page. The source never asks
 * `After`: it asks a dropped page again with its own `Before` request, and when the newest page comes back it
 * pages older until a page meets the window. Feed updates change the window in place. The messages, edits and
 * reactions of the user go to the product through `user_action`. The window shows an edit or a reaction before the
 * answer arrives, and a message once the product has it.
 *
 * The room shows no loading row and no failure: a page that did not
 * arrive is asked again, silently, after a delay that grows with each failure.
 *
 * All the state changes run on one thread at a time, so the code between two suspension points is atomic.
 */
@Suppress("TooManyFunctions")
class ProductMessageSource(
    private val chatId: ChatId,
    private val querying: ProductChatQuerying,
    private val cache: ProductMessageCaching,
    private val settings: Settings,
    private val now: () -> Long,
    dispatcher: CoroutineDispatcher,
) : ChatMessageSource, ProductFeedListener {
    data class Settings(
        val pageSize: Int,
        val windowLimit: Int,
        /** The wait before a page that the product could not give is asked again. It doubles with each failure. */
        val retryDelay: Duration,
        /** The most pages that one jump asks in each direction. */
        val jumpPageLimit: Int,
        /** The product wants a `Seen` action when the user sees a newer message. */
        val reportsSeen: Boolean,
    ) {
        companion object {
            val Default = Settings(pageSize = 50, windowLimit = 500, retryDelay = 1.seconds, jumpPageLimit = 1_000, reportsSeen = false)
        }
    }

    private sealed interface PageResult {
        data class Page(val item: ProductMessagesPage) : PageResult

        /** The product cannot give pages for this end (`NotAvailable`). */
        data object NotAvailable : PageResult

        /** The product did not answer in time, or refused the room. */
        data object Failed : PageResult

        /** The worker of the product does not answer. */
        data object Unavailable : PageResult
    }

    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    private val confined = dispatcher.limitedParallelism(1)
    private val scope = CoroutineScope(SupervisorJob() + confined)

    // A message, an edit or a reaction of the user reaches the product also when the user leaves the room at once.
    // [close] does not cancel it.
    private val userActions = CoroutineScope(SupervisorJob() + confined)

    private val extensionId = chatId.extensionOrNull()?.extensionId.orEmpty()
    private val window = ProductMessageWindow(settings.windowLimit)

    private var firstPageJob: Job? = null
    private var olderJob: Deferred<ChatPageOutcome>? = null
    private var newerJob: Deferred<ChatPageOutcome>? = null
    private var olderFailures = 0
    private var newerFailures = 0
    private var newestFailures = 0
    private var newestRetryJob: Job? = null
    private var readiness = ChatRoomReadiness.LOADING
    private var started = false
    private var seenToReport: String? = null
    private var isReportingSeen = false

    private val state = MutableStateFlow(window.snapshot(chatId, extensionId, paging(), readiness))

    /** The current window. */
    val snapshot: ChatFeedSnapshot
        get() = state.value

    override fun observeSnapshots(): Flow<ChatFeedSnapshot> = flow {
        start()
        emitAll(state)
    }

    /** Shows the cached page and asks for the newest page. A second call does nothing. */
    suspend fun start() = onSource {
        if (started) return@onSource
        started = true

        cache.entry(chatId)?.let(window::restore)
        emit()

        firstPageJob = scope.launch { loadNewestPage() }
    }

    /** Waits until the answer for the first page arrived. */
    suspend fun awaitFirstPage() {
        onSource { firstPageJob }?.join()
    }

    override suspend fun loadOlder(): ChatPageOutcome = onSource {
        olderJob?.let { return@onSource it.await() }
        val anchor = window.olderAnchor ?: return@onSource ChatPageOutcome.End

        val job = scope.async {
            delay(backoff(olderFailures))
            fetchOlder(anchor).also {
                olderJob = null
                emit()
            }
        }
        olderJob = job
        emit()
        job.await()
    }

    override suspend fun loadNewer(): ChatPageOutcome = onSource {
        newerJob?.let { return@onSource it.await() }
        val anchor = window.droppedNewer.lastOrNull() ?: return@onSource ChatPageOutcome.End

        val job = scope.async {
            delay(backoff(newerFailures))
            fetchNewer(anchor).also {
                newerJob = null
                emit()
            }
        }
        newerJob = job
        emit()
        job.await()
    }

    override suspend fun jump(messageId: ChatMessageId): Boolean {
        awaitFirstPage()
        return pageUntil(messageId, ::loadOlder) || pageUntil(messageId, ::loadNewer)
    }

    override suspend fun markUnavailable() = onSource {
        readiness = ChatRoomReadiness.UNAVAILABLE
        emit()
    }

    override suspend fun reload() {
        val job = onSource {
            newestRetryJob?.cancel()
            firstPageJob?.cancel()
            scope.launch { loadNewestPage() }.also { firstPageJob = it }
        }
        job.join()
    }

    override suspend fun perform(action: ChatSourceAction): ChatSourceActionOutcome = when (action) {
        is ChatSourceAction.Send -> userActions.async { send(action) }.await()
        is ChatSourceAction.Edit -> userActions.async { edit(action) }.await()
        is ChatSourceAction.ToggleReaction -> userActions.async { toggleReaction(action.messageId, action.emoji) }.await()
        is ChatSourceAction.MarkSeen -> onSource {
            markSeen(action.messageIds)
            ChatSourceActionOutcome.Done
        }
    }

    override suspend fun apply(chatId: ChatId, update: ProductFeedUpdateModel) {
        if (chatId != this.chatId) return

        onSource {
            applyUpdate(update)
            emit()
        }
    }

    override fun close() {
        scope.cancel()
    }

    private suspend fun <T> onSource(block: suspend () -> T): T = withContext(confined) { block() }

    // region Pages

    private suspend fun pageUntil(messageId: ChatMessageId, load: suspend () -> ChatPageOutcome): Boolean {
        repeat(settings.jumpPageLimit) {
            if (onSource { window.contains(messageId) }) return true
            if (load() !is ChatPageOutcome.Appended) return onSource { window.contains(messageId) }
        }
        return onSource { window.contains(messageId) }
    }

    private suspend fun loadNewestPage() {
        readiness = ChatRoomReadiness.LOADING
        emit()

        val result = ask(ChatPageAnchorModel.Before(null))
        readiness = when (result) {
            is PageResult.Page -> {
                window.replaceWith(result.item)
                newestFailures = 0
                ChatRoomReadiness.READY
            }

            // The window keeps the cached pages, and the newest page is asked again later.
            PageResult.Failed -> ChatRoomReadiness.FAILED
            PageResult.NotAvailable, PageResult.Unavailable -> ChatRoomReadiness.UNAVAILABLE
        }

        emit()
        storeCache()
        if (result !is PageResult.Page) retryNewestPageLater()
    }

    private fun retryNewestPageLater() {
        val wait = backoff(++newestFailures)
        newestRetryJob?.cancel()
        newestRetryJob = scope.launch {
            delay(wait)
            firstPageJob = scope.launch { loadNewestPage() }
        }
    }

    /** No wait before the first try, then [Settings.retryDelay], doubling with each failure up to [MAX_RETRY_DELAY]. */
    private fun backoff(failures: Int): Duration {
        if (failures <= 0) return Duration.ZERO
        val doubled = settings.retryDelay * (1 shl (failures - 1).coerceAtMost(MAX_BACKOFF_SHIFT))
        return doubled.coerceAtMost(MAX_RETRY_DELAY)
    }

    private suspend fun fetchOlder(anchor: ChatPageAnchorModel): ChatPageOutcome = when (val result = ask(anchor)) {
        is PageResult.Page -> {
            olderFailures = 0
            ChatPageOutcome.Appended(window.addOlder(anchor, result.item))
        }

        PageResult.NotAvailable -> {
            window.olderClosed = true
            ChatPageOutcome.End
        }

        PageResult.Failed -> {
            olderFailures++
            ChatPageOutcome.Failed
        }

        PageResult.Unavailable -> {
            olderFailures++
            readiness = ChatRoomReadiness.UNAVAILABLE
            ChatPageOutcome.Unavailable
        }
    }

    // The newest page can gain messages while it is out of the window. The source asks older pages from the
    // newest one until a page meets the window, so no message goes missing between them.
    private suspend fun fetchNewer(anchor: ChatPageAnchorModel): ChatPageOutcome {
        val chunk = mutableListOf<ProductMessageWindow.Page>()
        var request = anchor

        repeat(settings.jumpPageLimit) {
            when (val result = ask(request)) {
                is PageResult.Page -> {
                    val item = result.item
                    val page = ProductMessageWindow.Page(anchor = request, next = item.next, ids = item.messages.map { it.messageId })
                    chunk.add(0, page)

                    val asksNewest = request == ChatPageAnchorModel.Before(null) && window.droppedNewer.size == 1
                    if (!asksNewest || window.meetsWindow(page)) {
                        newerFailures = 0
                        return ChatPageOutcome.Appended(window.appendNewer(chunk, listOf(item)))
                    }

                    request = ChatPageAnchorModel.Before(item.next)
                    item.messages.forEach { window.messages[it.messageId] = it }
                }

                PageResult.NotAvailable,
                PageResult.Failed -> {
                    newerFailures++
                    return ChatPageOutcome.Failed
                }

                PageResult.Unavailable -> {
                    newerFailures++
                    readiness = ChatRoomReadiness.UNAVAILABLE
                    return ChatPageOutcome.Unavailable
                }
            }
        }

        window.restartFromNewest()
        newerFailures = 0
        return ChatPageOutcome.Appended(window.appendNewer(chunk, emptyList()))
    }

    /** Asks for one page, and asks once more after [Settings.retryDelay] when the product is not available. */
    private suspend fun ask(anchor: ChatPageAnchorModel): PageResult {
        var retried = false

        while (true) {
            val result = querying.messagesPage(chatId, anchor, settings.pageSize)
            result.onSuccess { return PageResult.Page(it) }

            val error = result.exceptionOrNull()
            if (error is CancellationException) throw error

            if (!retried && (isUnavailable(error) || error is ProductChatQueryException.NotAvailable)) {
                retried = true
                delay(settings.retryDelay)
                continue
            }

            Timber.w(error, "A page of a product room failed")
            return when {
                error is ProductChatQueryException.NotAvailable -> PageResult.NotAvailable
                isUnavailable(error) -> PageResult.Unavailable
                else -> PageResult.Failed
            }
        }
    }

    // endregion

    // region Feed updates

    private fun applyUpdate(update: ProductFeedUpdateModel) {
        when (update) {
            is ProductFeedUpdateModel.New -> window.insertLive(update.message)
            is ProductFeedUpdateModel.Edited -> window.modify(update.messageId) { it.copy(content = update.content, editedAt = update.editedAt) }
            is ProductFeedUpdateModel.Replaced -> window.modify(update.message.messageId) { update.message }
            is ProductFeedUpdateModel.ReactionPlaced ->
                window.modify(update.messageId) { it.adjustReaction(update.emoji, update.participantId, placed = true) }

            is ProductFeedUpdateModel.ReactionRemoved ->
                window.modify(update.messageId) { it.adjustReaction(update.emoji, update.participantId, placed = false) }

            is ProductFeedUpdateModel.StatusChanged -> window.modify(update.messageId) { it.copy(status = update.status) }
        }
    }

    // endregion

    // region User actions

    // The room shows its newest messages, and the message once the product has it.
    private suspend fun send(action: ChatSourceAction.Send): ChatSourceActionOutcome {
        if (onSource { !window.isAtNewestEnd }) reload()

        val result = querying.userAction(chatId, ChatUserActionModel.NewMessage(action.content, action.replyTo))
        val error = result.exceptionOrNull()
        if (error != null) {
            if (error is CancellationException) throw error
            Timber.w(error, "A message of a product room failed")
            return ChatSourceActionOutcome.Refused((error as? ProductChatQueryException.Rejected)?.reason)
        }

        (result.getOrNull() as? ChatUserActionOutcome.MessageCreated)?.let { created ->
            window.insertLive(created.message)
            emit()
        }
        return ChatSourceActionOutcome.Done
    }

    private suspend fun edit(action: ChatSourceAction.Edit): ChatSourceActionOutcome {
        val original = window.messages[action.messageId] ?: return ChatSourceActionOutcome.Refused(reason = null)

        val edited = (action.content as? ChatMessage.Content.Text)?.text?.let(original.content::withEditedText) ?: action.content
        window.modify(action.messageId) { it.copy(content = edited, editedAt = now()) }
        return askAction(ChatUserActionModel.EditMessage(action.messageId, action.content)) { window.put(original) }
    }

    private suspend fun toggleReaction(messageId: ChatMessageId, emoji: String): ChatSourceActionOutcome {
        val original = window.messages[messageId] ?: return ChatSourceActionOutcome.Refused(reason = null)
        val placed = original.reactions.none { it.emoji == emoji && it.reactedByUser }

        // As in a person chat, the user has one reaction on a message: a new emoji replaces the one before.
        window.modify(messageId) { message ->
            val others = message.reactions.filter { it.reactedByUser && it.emoji != emoji }.map { it.emoji }
            others.fold(message) { changed, other -> changed.adjustReaction(other, ProductParticipant.USER, placed = false) }
                .adjustReaction(emoji, ProductParticipant.USER, placed)
        }
        val action = if (placed) ChatUserActionModel.ReactionPlaced(messageId, emoji) else ChatUserActionModel.ReactionRemoved(messageId, emoji)
        return askAction(action) { window.put(original) }
    }

    /** Sends an action whose change the window shows already. [revert] undoes the change when the action fails. */
    private suspend fun askAction(action: ChatUserActionModel, revert: () -> Unit): ChatSourceActionOutcome {
        emit()

        val error = querying.userAction(chatId, action).exceptionOrNull() ?: return ChatSourceActionOutcome.Done
        if (error is CancellationException) throw error

        Timber.w(error, "A user action of a product room failed")
        revert()
        emit()
        return ChatSourceActionOutcome.Refused((error as? ProductChatQueryException.Rejected)?.reason)
    }

    // endregion

    // region Seen

    /** Notes the newest of the messages as seen, and tells the product when it asks for `Seen`. */
    private fun markSeen(messageIds: Set<ChatMessageId>) {
        val ids = window.orderedIds
        val newestIndex = ids.indexOfLast { it in messageIds }
        val newest = ids.getOrNull(newestIndex)?.let { window.messages[it] } ?: return

        if (isNewer(newest, window.seenMessageId, window.seenTimestamp, ids)) {
            window.seenMessageId = newest.messageId
            window.seenTimestamp = newest.timestamp
            emit()
        }

        if (!settings.reportsSeen) return

        val reported = window.reportedSeenMessageId
        if (isNewer(newest, reported, reported?.let { window.messages[it] }?.timestamp, ids)) {
            seenToReport = newest.messageId
            reportSeenIfNeeded()
        }
    }

    private fun isNewer(message: ProductRoomMessage, otherId: String?, otherTimestamp: Long?, ids: List<String>): Boolean {
        if (otherId == null) return true
        if (otherId == message.messageId) return false

        val index = ids.indexOf(message.messageId)
        val otherIndex = ids.indexOf(otherId)
        if (index != -1 && otherIndex != -1) return index > otherIndex

        return message.timestamp > (otherTimestamp ?: 0)
    }

    /** Sends one `Seen` at a time. A newer message that the user sees in the meantime goes next. */
    private fun reportSeenIfNeeded() {
        if (isReportingSeen) return
        val messageId = seenToReport ?: return

        isReportingSeen = true
        seenToReport = null

        scope.launch {
            querying.userAction(chatId, ChatUserActionModel.Seen(messageId))
                .onFailure { Timber.w(it, "A seen report of a product room failed") }

            window.reportedSeenMessageId = messageId
            isReportingSeen = false
            reportSeenIfNeeded()
            storeCache()
        }
    }

    // endregion

    private fun emit() {
        state.value = window.snapshot(chatId, extensionId, paging(), readiness)
    }

    // A page that failed is MORE again: the screen asks it when the user comes near its end, after the backoff.
    private fun paging(): ChatPagingState {
        val older = when {
            olderJob != null || (!window.hasFirstPage && readiness == ChatRoomReadiness.LOADING) -> ChatPagingState.Edge.LOADING
            window.olderAnchor != null -> ChatPagingState.Edge.MORE
            else -> ChatPagingState.Edge.END
        }

        val newer = when {
            newerJob != null -> ChatPagingState.Edge.LOADING
            window.droppedNewer.isNotEmpty() -> ChatPagingState.Edge.MORE
            else -> ChatPagingState.Edge.END
        }

        return ChatPagingState(older = older, newer = newer)
    }

    private suspend fun storeCache() {
        window.cacheEntry(settings.pageSize)?.let { cache.store(chatId, it) }
    }

    companion object {
        private val MAX_RETRY_DELAY = 60.seconds
        private const val MAX_BACKOFF_SHIFT = 6

        /** True for an error that means the worker of the product does not answer. */
        fun isUnavailable(error: Throwable?): Boolean = when (error) {
            is ProductChatQueryException.Failed,
            is ProductChatQueryException.NoAnswer,
            is ProductChatQueryException.Unavailable -> true

            is ProductChatQueryException -> false
            is CancellationException -> false
            else -> true
        }
    }
}
