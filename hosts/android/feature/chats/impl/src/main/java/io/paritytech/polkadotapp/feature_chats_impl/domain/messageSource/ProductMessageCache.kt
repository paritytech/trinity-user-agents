package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
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
}

/** Keeps the newest page of the last rooms in memory only. The room of the oldest use goes when the cache is full. */
@Singleton
class ProductMessageCache(private val capacity: Int) : ProductMessageCaching {
    @Inject
    constructor() : this(DEFAULT_CAPACITY)

    private val lock = Mutex()
    private val entries = LinkedHashMap<ChatId, ProductRoomCacheEntry>(capacity, LOAD_FACTOR, true)

    override suspend fun entry(chatId: ChatId): ProductRoomCacheEntry? = lock.withLock { entries[chatId] }

    override suspend fun store(chatId: ChatId, entry: ProductRoomCacheEntry) = lock.withLock {
        entries[chatId] = entry
        while (entries.size > capacity) entries.remove(entries.keys.first())
    }

    companion object {
        const val DEFAULT_CAPACITY = 32
        private const val LOAD_FACTOR = 0.75f
    }
}
