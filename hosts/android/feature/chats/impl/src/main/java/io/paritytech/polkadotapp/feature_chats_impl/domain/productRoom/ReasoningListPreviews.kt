package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatReasoningMessages
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

/** Reads the messages of a room as the room screen shows them, the newest first. */
fun interface RoomMessagesReader {
    suspend fun messages(chatId: ChatId): List<ChatMessage>
}

class ChatEngineRoomMessagesReader @Inject constructor(
    private val chatEngine: ChatEngine,
) : RoomMessagesReader {
    override suspend fun messages(chatId: ChatId): List<ChatMessage> = chatEngine.subscribeMessagesFeed(chatId).first()
}

/**
 * A reasoning row is never the list preview (07-LATER §9.4). When the last message of a room is a reasoning row, the
 * row of the room shows the newest message before it, or an empty room.
 */
class ReasoningListPreviews @Inject constructor(
    private val reasoningMessages: ChatReasoningMessages,
    private val roomMessages: RoomMessagesReader,
) {
    // The previous message changes only when the reasoning message changes, so a stream of edits reads the room once.
    private val replacements = ConcurrentHashMap<ChatId, Pair<ChatMessageId, ChatMessage?>>()

    fun apply(summaries: Flow<List<ChatSummary>>): Flow<List<ChatSummary>> =
        combine(summaries, reasoningMessages.observe()) { list, reasoning ->
            if (reasoning.isEmpty()) return@combine list
            list.map { summary -> summary.withoutReasoning(reasoning[summary.chatId].orEmpty()) }
        }

    private suspend fun ChatSummary.withoutReasoning(reasoningIds: Set<String>): ChatSummary {
        val preview = preview as? ChatPreview.Message ?: return this
        val last = preview.message
        if (reasoningIds.isEmpty() || ProductMessageIds.product(last.id, chatId) !in reasoningIds) return this

        val cached = replacements[chatId]?.takeIf { it.first == last.id }
        val previous = if (cached != null) {
            cached.second
        } else {
            roomMessages.messages(chatId)
                .firstOrNull { ProductMessageIds.product(it.id, chatId) !in reasoningIds }
                .also { replacements[chatId] = last.id to it }
        }
        return copy(preview = previous?.let { ChatPreview.Message(it, preview.order) } ?: ChatPreview.EmptyChat)
    }
}
