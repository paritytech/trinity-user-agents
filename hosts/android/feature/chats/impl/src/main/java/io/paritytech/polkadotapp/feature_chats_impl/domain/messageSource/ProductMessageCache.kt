package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/** What the host keeps of a product-source room after its room screen closes (02-DESIGN §5.3). */
data class ProductRoomCacheEntry(
    /** The newest page, the oldest message first. */
    val messages: List<ProductRoomMessage>,
    /** The cursor of the page before the newest page. */
    val next: String?,
    /** The last message that the user saw. */
    val seenMessageId: String?,
    val seenTimestamp: Long?,
    /** The last message that the host sent in a `Seen` action. */
    val reportedSeenMessageId: String?,
)

interface ProductMessageCaching {
    suspend fun entry(chatId: ChatId): ProductRoomCacheEntry?

    suspend fun store(chatId: ChatId, entry: ProductRoomCacheEntry)

    /**
     * Changes the cached page of a room that no room screen shows, as an open screen changes its window (02-DESIGN
     * §5.3 step 3). A room with no cached page stays without one.
     */
    suspend fun apply(chatId: ChatId, update: ProductFeedUpdateModel)
}

/** Keeps the newest page of the last rooms in memory only. The room of the oldest use goes when the cache is full. */
@Singleton
class ProductMessageCache(
    private val capacity: Int,
    /** The most messages of one cached page. A new message past it drops the oldest one. */
    private val messageLimit: Int = DEFAULT_MESSAGE_LIMIT,
) : ProductMessageCaching {
    @Inject
    constructor() : this(DEFAULT_CAPACITY)

    private val lock = Mutex()
    private val entries = LinkedHashMap<ChatId, ProductRoomCacheEntry>(capacity, LOAD_FACTOR, true)

    override suspend fun entry(chatId: ChatId): ProductRoomCacheEntry? = lock.withLock { entries[chatId] }

    override suspend fun store(chatId: ChatId, entry: ProductRoomCacheEntry) = lock.withLock {
        entries[chatId] = entry
        while (entries.size > capacity) entries.remove(entries.keys.first())
    }

    override suspend fun apply(chatId: ChatId, update: ProductFeedUpdateModel) = lock.withLock {
        val entry = entries[chatId] ?: return@withLock
        entries[chatId] = entry.applying(update, messageLimit)
    }

    companion object {
        const val DEFAULT_CAPACITY = 32

        /** The page size of a product-source room, so a cached page holds one page. */
        const val DEFAULT_MESSAGE_LIMIT = 50
        private const val LOAD_FACTOR = 0.75f
    }
}

/**
 * The entry after a feed update, with the rules of the window of an open room (02-DESIGN §3.3.4). A new message goes
 * after the last message that is not newer. When the page holds more than [messageLimit] messages, the oldest messages
 * go and the page has no cursor: the next open of the room asks the newest page again and gets the cursor back.
 */
internal fun ProductRoomCacheEntry.applying(update: ProductFeedUpdateModel, messageLimit: Int): ProductRoomCacheEntry {
    fun modify(messageId: String, change: (ProductRoomMessage) -> ProductRoomMessage) =
        messages.map { if (it.messageId == messageId) change(it) else it }

    return when (update) {
        is ProductFeedUpdateModel.New -> {
            val message = update.message
            val changed = messages.toMutableList()
            val known = changed.indexOfFirst { it.messageId == message.messageId }
            if (known >= 0) {
                changed[known] = message
            } else {
                changed.add(changed.indexOfLast { it.timestamp <= message.timestamp } + 1, message)
            }
            if (changed.size > messageLimit) {
                copy(messages = changed.takeLast(messageLimit), next = null)
            } else {
                copy(messages = changed)
            }
        }

        is ProductFeedUpdateModel.Edited ->
            copy(messages = modify(update.messageId) { it.copy(content = update.content, editedAt = update.editedAt) })

        is ProductFeedUpdateModel.Replaced -> copy(messages = modify(update.message.messageId) { update.message })

        is ProductFeedUpdateModel.ReactionPlaced ->
            copy(messages = modify(update.messageId) { it.adjustReaction(update.emoji, update.participantId, placed = true) })

        is ProductFeedUpdateModel.ReactionRemoved ->
            copy(messages = modify(update.messageId) { it.adjustReaction(update.emoji, update.participantId, placed = false) })

        is ProductFeedUpdateModel.StatusChanged -> copy(messages = modify(update.messageId) { it.copy(status = update.status) })
    }
}
