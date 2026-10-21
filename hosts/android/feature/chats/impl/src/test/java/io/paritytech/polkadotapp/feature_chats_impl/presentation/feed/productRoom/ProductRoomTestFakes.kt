package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.ChatReadAloudController
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** A product room with no additions, or with the additions that a test sets. */
class FakeProductRoomAdditionsFeed(
    val personas: MutableStateFlow<List<ChatBotPersonaModel>> = MutableStateFlow(emptyList()),
) : ProductRoomAdditionsFeed {
    val published = mutableListOf<Pair<ChatId, ChatRoomEventModel>>()

    override fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>> = personas

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
