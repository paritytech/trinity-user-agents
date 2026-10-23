package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

/**
 * The pages of a product-source room that the room screen shows, the oldest page first. The window holds at most
 * [windowLimit] messages, so a new page at one end drops whole pages at the other end. The window keeps the
 * request of each dropped page, and the source asks it again when the user comes back. Only one coroutine at a
 * time uses a window.
 */
internal class ProductMessageWindow(private val windowLimit: Int) {
    data class Page(
        /** The request that gave the page. The source asks it again after the window drops the page. */
        val anchor: ChatPageAnchorModel,
        /** The cursor of the page before this one. Null when the product has no older message. */
        val next: String?,
        val ids: List<String>,
    )

    val pages = mutableListOf<Page>()
    val messages = mutableMapOf<String, ProductRoomMessage>()

    /** The pages that the window dropped at each end. The last page is next to the window. */
    val droppedOlder = mutableListOf<ChatPageAnchorModel>()
    val droppedNewer = mutableListOf<ChatPageAnchorModel>()

    /** The product cannot give older pages. */
    var olderClosed = false
    var hasFirstPage = false

    var seenMessageId: String? = null
    var seenTimestamp: Long? = null
    var reportedSeenMessageId: String? = null

    val isAtNewestEnd: Boolean
        get() = droppedNewer.isEmpty()

    val windowCount: Int
        get() = pages.sumOf { it.ids.size }

    val orderedIds: List<String>
        get() = pages.flatMap { it.ids }

    /** The request for the page before the window, or null when the room has no older message. */
    val olderAnchor: ChatPageAnchorModel?
        get() {
            droppedOlder.lastOrNull()?.let { return it }
            val next = pages.firstOrNull()?.next
            return if (hasFirstPage && !olderClosed && next != null) ChatPageAnchorModel.Before(next) else null
        }

    fun contains(messageId: String): Boolean = pages.any { messageId in it.ids }

    fun replaceWith(item: ProductMessagesPage) {
        messages.clear()
        droppedOlder.clear()
        droppedNewer.clear()
        olderClosed = false

        val ids = mutableListOf<String>()
        for (message in item.messages) {
            if (messages.containsKey(message.messageId)) continue
            messages[message.messageId] = message
            ids += message.messageId
        }

        pages.clear()
        pages += Page(anchor = ChatPageAnchorModel.Before(null), next = item.next, ids = ids)

        hasFirstPage = true
        trimNewestMessages()
    }

    /** Adds a page before the window and gives the number of new messages. */
    fun addOlder(anchor: ChatPageAnchorModel, item: ProductMessagesPage): Int {
        if (droppedOlder.lastOrNull() == anchor) droppedOlder.removeAt(droppedOlder.lastIndex)

        val ids = add(item.messages)
        pages.add(0, Page(anchor = anchor, next = item.next, ids = ids))
        dropNewerPagesIfNeeded()
        return ids.size
    }

    /** Adds pages after the window, the oldest first, and gives the number of new messages. */
    fun appendNewer(chunk: List<Page>, items: List<ProductMessagesPage>): Int {
        droppedNewer.removeLastOrNull()
        items.flatMap { it.messages }.forEach { messages[it.messageId] = it }

        var added = 0
        for (page in chunk) {
            val ids = page.ids.filter { id -> pages.none { id in it.ids } }
            added += ids.size
            pages += page.copy(ids = ids)
        }

        dropOlderPagesIfNeeded()
        return added
    }

    /** True when [page], the page before a newer page, touches the newest page of the window. */
    fun meetsWindow(page: Page): Boolean {
        val newestWindowPage = pages.lastOrNull() ?: return true
        if (page.next == null || page.ids.any { it in newestWindowPage.ids }) return true

        val anchor = newestWindowPage.anchor
        return anchor is ChatPageAnchorModel.Before && anchor.cursor == page.next
    }

    /** The newer pages did not meet the window. The window starts again from the newest pages. */
    fun restartFromNewest() {
        droppedOlder.clear()
        droppedNewer.clear()
        pages.clear()
    }

    fun insertLive(message: ProductRoomMessage) {
        if (contains(message.messageId)) {
            messages[message.messageId] = message
            return
        }

        // The newest pages are out of the window. The message comes back with them.
        if (!isAtNewestEnd) return

        if (pages.isEmpty()) pages += Page(anchor = ChatPageAnchorModel.Before(null), next = null, ids = emptyList())

        // A new message goes last whatever its time, as the product orders it and as the native chat places a message
        // of a contact session (base ChatMessagePlacement.Latest).
        val page = pages.last()
        pages[pages.lastIndex] = page.copy(ids = page.ids + message.messageId)
        messages[message.messageId] = message

        dropOlderPagesIfNeeded()
    }

    fun modify(messageId: String, change: (ProductRoomMessage) -> ProductRoomMessage) {
        val message = messages[messageId] ?: return
        messages[messageId] = change(message)
    }

    /** Puts back the earlier state of a message that is still in the window. */
    fun put(message: ProductRoomMessage) {
        if (messages.containsKey(message.messageId)) messages[message.messageId] = message
    }

    fun restore(entry: ProductRoomCacheEntry) {
        entry.messages.forEach { messages[it.messageId] = it }
        pages.clear()
        pages += Page(anchor = ChatPageAnchorModel.Before(null), next = entry.next, ids = entry.messages.map { it.messageId })
        seenMessageId = entry.seenMessageId
        seenTimestamp = entry.seenTimestamp
        reportedSeenMessageId = entry.reportedSeenMessageId
    }

    fun cacheEntry(pageSize: Int): ProductRoomCacheEntry? {
        val newest = pages.lastOrNull()
        if (!hasFirstPage || !isAtNewestEnd || newest == null) return null

        return ProductRoomCacheEntry(
            messages = newest.ids.takeLast(pageSize).mapNotNull { messages[it] },
            next = if (newest.ids.size > pageSize) null else newest.next,
            seenMessageId = seenMessageId,
            seenTimestamp = seenTimestamp,
            reportedSeenMessageId = reportedSeenMessageId,
        )
    }

    fun snapshot(chatId: ChatId, extensionId: ChatExtensionId, paging: ChatPagingState, readiness: ChatRoomReadiness): ChatFeedSnapshot {
        val ids = orderedIds
        val seenIndex = seenMessageId?.let { ids.indexOf(it) }?.takeIf { it != -1 }
        val seenAt = seenTimestamp
        val windowMessages = ids.mapIndexedNotNull { index, id -> messages[id]?.let { index to it } }

        return ChatFeedSnapshot(
            messages = windowMessages.asReversed().map { (index, message) ->
                // The product reads an incoming message once the user saw it; until it says so, the screen keeps what the
                // user saw here.
                val seenHere = when {
                    seenIndex != null -> index <= seenIndex
                    seenAt != null -> message.timestamp <= seenAt
                    else -> false
                }
                val isUnread = message.status != ProductMessageStatusModel.READ && !seenHere
                message.toChatMessage(chatId, extensionId, isUnread)
            },
            reactions = windowMessages.associate { (_, message) -> message.messageId to message.toReactions(extensionId) }
                .filterValues { it.isNotEmpty() },
            revisions = windowMessages.mapNotNull { (_, message) -> message.toRevision(chatId)?.let { message.messageId to it } }.toMap(),
            productStatuses = windowMessages.filter { (_, message) -> message.isFromUser }
                .associate { (_, message) -> message.messageId to message.status },
            paging = paging,
            readiness = readiness,
            replyQuotes = windowMessages.mapNotNull { (_, message) ->
                val repliedId = message.replyTo?.takeUnless { it in messages } ?: return@mapNotNull null
                message.replyPreview?.let { repliedId to it.toQuotedMessage(repliedId, chatId, extensionId) }
            }.toMap(),
        )
    }

    /** Adds the messages of a page that are not in the window yet, and gives their ids. */
    private fun add(newMessages: List<ProductRoomMessage>): List<String> = newMessages.mapNotNull { message ->
        if (contains(message.messageId)) return@mapNotNull null
        messages[message.messageId] = message
        message.messageId
    }

    private fun dropNewerPagesIfNeeded() {
        while (windowCount > windowLimit && pages.size > 1) {
            val page = pages.removeAt(pages.lastIndex)
            droppedNewer += page.anchor
            page.ids.forEach { messages.remove(it) }
        }
    }

    private fun dropOlderPagesIfNeeded() {
        while (windowCount > windowLimit && pages.size > 1) {
            val page = pages.removeAt(0)
            droppedOlder += page.anchor
            page.ids.forEach { messages.remove(it) }
        }

        trimNewestMessages()
    }

    // A single page that grew past the limit with live messages loses its oldest messages. The page keeps its
    // cursor, so a later older page can skip the dropped messages.
    private fun trimNewestMessages() {
        val page = pages.lastOrNull() ?: return
        val excess = windowCount - windowLimit
        if (excess <= 0) return

        page.ids.take(excess).forEach { messages.remove(it) }
        pages[pages.lastIndex] = page.copy(ids = page.ids.drop(excess))
    }
}
