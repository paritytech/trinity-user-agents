package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageMetaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.AlwaysFailCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCaching
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** The message of another room that a thread room answers, as the thread room shows it at its top (07-LATER §9.2). */
data class ChatThreadRootModel(
    val parentChatId: ChatId,
    val parentName: String?,
    /** The text of the root message. Null when the host does not have the message, or the message has no text. */
    val rootText: ChatQuoteText?,
)

/** The text of a message for a quote. A [markdown] text is the rich text of a product. */
data class ChatQuoteText(val text: String, val markdown: Boolean)

/**
 * What the room screen reads of the Chat Extension v2 additions of a product room (07-LATER §5, §9): the personas of
 * the product, the meta of the messages, the messages that are `Preparing`, the unread state of the thread rooms and
 * the root of a thread. It also sends the room events of the screen to the product.
 */
interface ProductRoomAdditionsFeed {
    fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>>

    /** The meta of each message of the room, by the message id of the product. */
    fun messageMeta(chatId: ChatId): Flow<Map<String, ChatMessageMetaModel>>

    /** The product ids of the messages of a host-store room whose status is `Preparing`. */
    fun preparingMessages(chatId: ChatId): Flow<Set<String>>

    /** The rooms of [chatIds] that have unread messages. */
    fun roomsWithUnread(chatIds: Set<ChatId>): Flow<Set<ChatId>>

    suspend fun threadRoot(chatId: ChatId, parent: ChatThreadParentModel): ChatThreadRootModel

    suspend fun publish(chatId: ChatId, event: ChatRoomEventModel)
}

class RealProductRoomAdditionsFeed @Inject constructor(
    private val personaRepository: ChatBotPersonaRepository,
    private val metaRepository: ChatMessageMetaRepository,
    private val preparing: PreparingProductMessages,
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val productMessageCache: ProductMessageCaching,
    private val eventPublisher: ChatRoomEventPublisher,
) : ProductRoomAdditionsFeed {
    override fun personas(chatId: ChatId): Flow<List<ChatBotPersonaModel>> {
        val extensionId = chatId.extensionOrNull()?.extensionId ?: return flowOf(emptyList())
        return personaRepository.observe(extensionId)
    }

    override fun messageMeta(chatId: ChatId): Flow<Map<String, ChatMessageMetaModel>> = metaRepository.observe(chatId)

    override fun preparingMessages(chatId: ChatId): Flow<Set<String>> = preparing.observe(chatId)

    override fun roomsWithUnread(chatIds: Set<ChatId>): Flow<Set<ChatId>> {
        if (chatIds.isEmpty()) return flowOf(emptySet())
        return chatRoomRepository.subscribeChatSummaries()
            .map { rooms ->
                rooms.filter { it.unseenCount > 0 }.mapNotNullTo(mutableSetOf()) { row -> ChatId.fromRawValue(row.chatId).takeIf { it in chatIds } }
            }
            .distinctUntilChanged()
    }

    /**
     * The root of a product-source parent comes from the page that the host keeps of the parent room. A root outside
     * that page has no text, and the screen shows "Earlier message".
     */
    override suspend fun threadRoot(chatId: ChatId, parent: ChatThreadParentModel): ChatThreadRootModel {
        val extensionId = chatId.extensionOrNull()?.extensionId
        val parentChatId = if (extensionId == null) chatId else ChatId.forExtensionRoom(extensionId, parent.roomId)
        val parentName = chatRoomRepository.getRoomMetadata(parentChatId).name
        val productSource = declarationsRepository.get(parentChatId)?.effectiveConfig?.productSource == true

        val rootText = if (productSource) {
            productMessageCache.entry(parentChatId)?.messages?.find { it.messageId == parent.messageId }?.content?.quoteText()
        } else {
            ProductMessageIds.candidates(parent.messageId, parentChatId).firstNotNullOfOrNull { candidate ->
                chatMessageRepository.getMessageById(parentChatId, candidate, AlwaysFailCustomContentDecoder())?.takeIf { it.chatId == parentChatId }
            }?.content?.quoteText()
        }
        return ChatThreadRootModel(parentChatId, parentName, rootText)
    }

    override suspend fun publish(chatId: ChatId, event: ChatRoomEventModel) = eventPublisher.publish(chatId, event)
}

/** The text that a quote of a message shows, or null for a message without text. */
internal fun ChatMessage.Content.quoteText(): ChatQuoteText? {
    val quote = when (this) {
        is ChatMessage.Content.Text -> ChatQuoteText(text, markdown = false)
        is ChatMessage.Content.RichText -> text?.let { ChatQuoteText(it, markdown = true) }
        is ChatMessage.Content.ProductBody -> when (val body = body) {
            is ProductMessageBody.RichText -> body.text?.let { ChatQuoteText(it, markdown = true) }
            is ProductMessageBody.Actions -> body.text?.let { ChatQuoteText(it, markdown = true) }
        }
        else -> null
    }
    return quote?.takeIf { it.text.isNotBlank() }
}
