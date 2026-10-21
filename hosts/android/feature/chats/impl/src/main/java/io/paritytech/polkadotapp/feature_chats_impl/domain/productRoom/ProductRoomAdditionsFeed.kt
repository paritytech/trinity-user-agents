package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

/**
 * What the room screen reads of the Chat Extension v2 additions of a product room (07-LATER §5): the personas of the
 * product. It also sends the room events of the screen to the product.
 */
interface ProductRoomAdditionsFeed {
    fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>>

    suspend fun publish(chatId: ChatId, event: ChatRoomEventModel)
}

class RealProductRoomAdditionsFeed @Inject constructor(
    private val personaRepository: ChatBotPersonaRepository,
    private val eventPublisher: ChatRoomEventPublisher,
) : ProductRoomAdditionsFeed {
    override fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>> {
        val extensionId = chatId.extensionOrNull()?.extensionId ?: return flowOf(emptyList())
        return personaRepository.observe(extensionId)
    }

    override suspend fun publish(chatId: ChatId, event: ChatRoomEventModel) = eventPublisher.publish(chatId, event)
}
