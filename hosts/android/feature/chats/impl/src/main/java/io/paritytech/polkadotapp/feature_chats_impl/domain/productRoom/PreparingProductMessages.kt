package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The messages of the products in host-store rooms whose status is `Preparing`, by their product ids. The host
 * database does not keep the status of a message of a product, so a reasoning row reads it from here (07-LATER §9.4).
 * The set is in memory only: after a restart, no message of a host-store room is `Preparing`.
 */
@Singleton
class PreparingProductMessages @Inject constructor() {
    private val preparing = MutableStateFlow<Map<ChatId, Set<String>>>(emptyMap())

    fun observe(chatId: ChatId): Flow<Set<String>> = preparing.map { it[chatId].orEmpty() }.distinctUntilChanged()

    fun set(chatId: ChatId, productMessageId: String, isPreparing: Boolean) = preparing.update { rooms ->
        val current = rooms[chatId].orEmpty()
        val next = if (isPreparing) current + productMessageId else current - productMessageId
        if (next.isEmpty()) rooms - chatId else rooms + (chatId to next)
    }

    fun clearRoom(chatId: ChatId) = preparing.update { it - chatId }
}
