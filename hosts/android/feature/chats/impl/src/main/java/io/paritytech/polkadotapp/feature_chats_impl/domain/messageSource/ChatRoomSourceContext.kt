package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject

/** What the source context of one room needs from the engine. */
interface ChatRoomServices {
    fun hostStoreSource(chatId: ChatId): ChatMessageSource
}

class EngineChatRoomServices @Inject constructor(
    private val chatEngine: ChatEngine,
) : ChatRoomServices {
    override fun hostStoreSource(chatId: ChatId): ChatMessageSource = HostStoreMessageSource(chatId, chatEngine)
}

class ChatRoomSourceContextFactory @Inject constructor(
    private val services: EngineChatRoomServices,
) {
    fun create(chatId: ChatId) = ChatRoomSourceContext(chatId, services)
}

/** Gives the message source of one room while the room screen shows it. */
class ChatRoomSourceContext(
    private val chatId: ChatId,
    private val services: ChatRoomServices,
) {
    private val source: ChatMessageSource by lazy { services.hostStoreSource(chatId) }

    fun source(): ChatMessageSource = source

    /** The source of the room. */
    fun observeSource(): Flow<ChatMessageSource> = flow { emit(source()) }

    /** Stops the source. Call it once, when the room screen goes away. */
    fun release() {
        source.close()
    }
}
