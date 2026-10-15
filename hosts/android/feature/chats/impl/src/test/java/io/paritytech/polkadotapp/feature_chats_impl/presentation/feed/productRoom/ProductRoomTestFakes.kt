package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatQuoteText
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatThreadRootModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.ChatReadAloudController
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map

/** A product room with no additions, or with the additions that a test sets. */
class FakeProductRoomAdditionsFeed(
    val personas: MutableStateFlow<List<ChatBotPersonaModel>> = MutableStateFlow(emptyList()),
    val meta: MutableStateFlow<Map<String, ChatMessageMetaModel>> = MutableStateFlow(emptyMap()),
    val preparing: MutableStateFlow<Set<String>> = MutableStateFlow(emptySet()),
    val unread: MutableStateFlow<Set<ChatId>> = MutableStateFlow(emptySet()),
    var root: ChatQuoteText? = null,
    var parentName: String? = null,
) : ProductRoomAdditionsFeed {
    val published = mutableListOf<Pair<ChatId, ChatRoomEventModel>>()

    override fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>> = personas

    override fun messageMeta(chatId: ChatId): Flow<Map<String, ChatMessageMetaModel>> = meta

    override fun preparingMessages(chatId: ChatId): Flow<Set<String>> = preparing

    override fun roomsWithUnread(chatIds: Set<ChatId>): Flow<Set<ChatId>> = unread.map { it intersect chatIds }

    override suspend fun threadRoot(chatId: ChatId, parent: ChatThreadParentModel) = ChatThreadRootModel(chatId, parentName, root)

    override suspend fun publish(chatId: ChatId, event: ChatRoomEventModel) {
        published += chatId to event
    }
}

class FakeReadAloudController : ChatReadAloudController {
    private val _speaking = MutableStateFlow<ChatId?>(null)
    override val speaking: StateFlow<ChatId?> = _speaking
    val spoken = mutableListOf<String>()

    override fun speak(chatId: ChatId, text: String) {
        spoken += text
        _speaking.value = chatId
    }

    override fun stop(chatId: ChatId) {
        if (_speaking.value == chatId) _speaking.value = null
    }
}
