package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import javax.inject.Inject

/** A stored message that can match an in-room search, with the text that the search compares. */
class ChatRoomSearchCandidate(
    val messageId: ChatMessageId,
    val timestamp: Long,
    val text: String,
    /** True when the user of this device wrote the message. */
    val byUser: Boolean,
    /** The participant of a product room that wrote the message. Null elsewhere. */
    val participantId: String?,
)

/** Reads the stored messages of one room for the in-room search of a room that the host keeps (07-LATER §8.2). */
interface ChatRoomSearchCandidates {
    /**
     * At most [limit] messages of [chatId], newest first, older than [before] (a timestamp and a message id). A
     * null [before] starts at the newest message. Each message that contains [foldedQuery] without case and
     * diacritics is in the answer, with other messages that the caller must check again.
     */
    suspend fun candidates(chatId: ChatId, foldedQuery: String, before: Pair<Long, ChatMessageId>?, limit: Int): List<ChatRoomSearchCandidate>
}

class RealChatRoomSearchCandidates @Inject constructor(
    private val chatMessageDao: ChatMessageDao,
) : ChatRoomSearchCandidates {
    override suspend fun candidates(
        chatId: ChatId,
        foldedQuery: String,
        before: Pair<Long, ChatMessageId>?,
        limit: Int,
    ): List<ChatRoomSearchCandidate> {
        return chatMessageDao.searchMessageCandidatesInChat(
            chatId = chatId.value.value,
            likeQuery = foldedQuery.escapeLikeWildcards(),
            beforeTimestamp = before?.first ?: Long.MAX_VALUE,
            beforeId = before?.second.orEmpty(),
            limit = limit,
        ).map { row ->
            ChatRoomSearchCandidate(
                messageId = row.id,
                timestamp = row.timestamp,
                text = row.searchableContent,
                byUser = row.originType == ChatMessageLocal.OriginType.USER,
                participantId = row.originParticipantId,
            )
        }
    }
}

private fun String.escapeLikeWildcards(): String = replace("\\", "\\\\")
    .replace("%", "\\%")
    .replace("_", "\\_")
