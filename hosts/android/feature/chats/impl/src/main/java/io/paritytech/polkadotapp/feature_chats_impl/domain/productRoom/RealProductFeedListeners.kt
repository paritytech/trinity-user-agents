package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListener
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRegistration
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCaching
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The room screens that show a product-source room now. An update for a room that no screen shows changes the cached
 * page of the room, so the next open shows it before the newest page arrives (VERBOSITY-WASM-PRODUCT gap 13).
 */
@Singleton
class RealProductFeedListeners @Inject constructor(
    private val cache: ProductMessageCaching,
) : ProductFeedListeners {
    private class Entry(val chatId: ChatId, val listener: ProductFeedListener)

    private val entries = MutableStateFlow<List<Entry>>(emptyList())

    override fun register(chatId: ChatId, listener: ProductFeedListener): ProductFeedRegistration {
        val entry = Entry(chatId, listener)
        entries.update { it + entry }
        val cancelled = AtomicBoolean(false)
        return ProductFeedRegistration {
            if (cancelled.compareAndSet(false, true)) entries.update { current -> current.filterNot { it === entry } }
        }
    }

    override suspend fun deliver(chatId: ChatId, update: ProductFeedUpdateModel): Boolean {
        val listeners = entries.value.filter { it.chatId == chatId }
        listeners.forEach { it.listener.apply(chatId, update) }
        if (listeners.isEmpty()) cache.apply(chatId, update)
        return listeners.isNotEmpty()
    }
}
