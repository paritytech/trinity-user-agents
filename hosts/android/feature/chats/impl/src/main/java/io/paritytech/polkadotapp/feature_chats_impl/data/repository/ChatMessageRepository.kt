package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao.MessageContentUpdateLocal
import io.paritytech.polkadotapp.database.dao.ChatMessageReactionDao
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.*
import io.paritytech.polkadotapp.feature_chats_impl.data.mappers.toLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatMessageSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastMessageSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocalType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import javax.inject.Inject
import kotlin.reflect.KClass
import kotlin.time.Instant

interface ChatMessageRepository {
    /** [onSaved] runs inside the write transaction — see [io.paritytech.polkadotapp.database.dao.ChatMessageDao]. */
    suspend fun saveMessage(
        chatMessage: ChatMessage,
        customContentDecoder: CustomContentDecoder,
        onConflict: ChatMessageSaveConflictStrategy = ChatMessageSaveConflictStrategy.REPLACE,
        placement: ChatMessagePlacement,
        onSaved: suspend () -> Unit = {},
    ): Boolean

    /**
     * Persists [chatMessages] in a single batch and returns the subset that was actually written.
     * With [ChatMessageSaveConflictStrategy.IGNORE] messages already present are skipped and excluded
     * from the result; with [ChatMessageSaveConflictStrategy.REPLACE] every message is overwritten and returned.
     */
    suspend fun saveMessages(
        chatMessages: List<ChatMessage>,
        customContentDecoder: CustomContentDecoder,
        onConflict: ChatMessageSaveConflictStrategy = ChatMessageSaveConflictStrategy.REPLACE,
        placement: ChatMessagePlacement,
    ): List<ChatMessage>

    suspend fun updateMessageStatus(messageId: ChatMessageId, status: ChatMessage.Status)

    suspend fun updateMessageContent(
        messageId: ChatMessageId,
        content: ChatMessage.Content,
        customContentDecoder: CustomContentDecoder,
    )

    suspend fun updateMessagesContents(
        modifications: List<Pair<ChatMessageId, ChatMessage.Content>>,
        customContentDecoder: CustomContentDecoder
    )

    suspend fun updateMessagesStatus(messageIds: List<ChatMessageId>, status: ChatMessage.Status)
    suspend fun updateOutgoingMessagesStatusForChat(chatId: ChatId, fromStatus: ChatMessage.Status, toStatus: ChatMessage.Status)
    suspend fun updateOutgoingMessagesStatusForChatWithTypes(chatId: ChatId, fromStatus: ChatMessage.Status, toStatus: ChatMessage.Status, contentTypes: List<KClass<out ChatMessage.Content>>)
    suspend fun updateOutgoingMessagesStatusForChatExcludingTypes(chatId: ChatId, fromStatus: ChatMessage.Status, toStatus: ChatMessage.Status, contentTypes: List<KClass<out ChatMessage.Content>>)

    suspend fun shiftChatMessagesToTimestamp(chatId: ChatId, timestamp: Timestamp)
    suspend fun removeMessage(messageId: ChatMessageId)

    suspend fun markMessagesAsReadUpToTimestamp(chatId: ChatId, timestamp: Timestamp)
    suspend fun markMessagesByTypesAsRead(chatId: ChatId, types: List<String>)
    suspend fun markMessageAsRead(messageId: ChatMessageId)

    fun subscribeLastMessageSummaries(
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<LastMessageSummary>>

    fun subscribeMessages(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<ChatMessage>>

    suspend fun getMessages(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder,
    ): List<ChatMessage>

    suspend fun getMessagesUpdatedAfter(
        after: Instant,
        customContentDecoder: CustomContentDecoder,
    ): List<ChatMessage>

    suspend fun getMessageById(
        chatId: ChatId,
        messageId: ChatMessageId,
        customContentDecoder: CustomContentDecoder
    ): ChatMessage?

    fun subscribeMessages(
        chatId: ChatId,
        direction: ChatMessageDirection,
        type: KClass<out ChatMessage.Content>,
        status: ChatMessage.Status,
        customContentDecoder: CustomContentDecoder
    ): Flow<List<ChatMessage>>

    fun subscribeOutgoingMessagesByStatus(
        chatId: ChatId,
        status: ChatMessage.Status,
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<ChatMessage>>

    fun subscribeMessageById(chatMessageId: ChatMessageId, customContentDecoder: CustomContentDecoder): Flow<ChatMessage?>

    fun subscribeCallSignalingMessages(customContentDecoder: CustomContentDecoder): Flow<List<ChatMessage>>

    fun subscribeMessageStatus(chatMessageId: ChatMessageId): Flow<ChatMessage.Status?>

    fun subscribeReactions(chatId: ChatId): Flow<List<ChatMessageReaction>>

    suspend fun addReaction(reaction: ChatMessageReaction, chatId: ChatId)

    suspend fun removeMessageReaction(reaction: ChatMessageReaction, chatId: ChatId)

    suspend fun getUnsupportedMessages(chatId: ChatId): List<ChatMessage>
    suspend fun deleteAllChatMessages(chatId: ChatId)

    suspend fun getMessageStatuses(messageIds: List<ChatMessageId>): Map<ChatMessageId, ChatMessage.Status>

    suspend fun commitCompaction(commitId: ChatMessageId, originalIds: List<ChatMessageId>)

    suspend fun propagateStatusToCompactedMessages(commitId: ChatMessageId, status: ChatMessage.Status)

    suspend fun unwindUnackedCompactions(chatId: ChatId)

    suspend fun searchMessages(query: String, chatIds: List<ChatId>): Result<List<ChatMessageSearchHit>>
}

class RealChatMessageRepository @Inject constructor(
    private val chatMessageDao: ChatMessageDao,
    private val chatMessageCompactionDao: ChatMessageCompactionDao,
    private val chatMessageReactionDao: ChatMessageReactionDao,
    private val chatRoomRepository: ChatRoomRepository,
    private val coroutineDispatchers: CoroutineDispatchers
) : ChatMessageRepository {
    override suspend fun saveMessage(
        chatMessage: ChatMessage,
        customContentDecoder: CustomContentDecoder,
        onConflict: ChatMessageSaveConflictStrategy,
        placement: ChatMessagePlacement,
        onSaved: suspend () -> Unit,
    ): Boolean {
        val local = chatMessage.toLocal(customContentDecoder)

        return when (onConflict) {
            ChatMessageSaveConflictStrategy.REPLACE -> {
                chatMessageDao.saveMessage(local, placement.toLocal(), onSaved)
                true
            }

            ChatMessageSaveConflictStrategy.IGNORE -> {
                chatMessageDao.saveMessageIfNotExists(local, placement.toLocal()).also { if (it >= 0) onSaved() } >= 0
            }
        }
    }

    override suspend fun saveMessages(
        chatMessages: List<ChatMessage>,
        customContentDecoder: CustomContentDecoder,
        onConflict: ChatMessageSaveConflictStrategy,
        placement: ChatMessagePlacement,
    ): List<ChatMessage> {
        if (chatMessages.isEmpty()) return emptyList()

        val locals = chatMessages.map { it.toLocal(customContentDecoder) }

        return when (onConflict) {
            ChatMessageSaveConflictStrategy.REPLACE -> {
                chatMessageDao.saveMessages(locals, placement.toLocal())
                chatMessages
            }

            ChatMessageSaveConflictStrategy.IGNORE -> {
                val rowIds = chatMessageDao.saveMessagesIfNotExist(locals, placement.toLocal())
                chatMessages.filterIndexed { index, _ -> rowIds[index] >= 0 }
            }
        }
    }

    override suspend fun updateMessageStatus(
        messageId: ChatMessageId,
        status: ChatMessage.Status
    ) {
        chatMessageDao.updateStatus(messageId, status.toLocal(), updatedAt = System.currentTimeMillis())
    }

    override suspend fun updateMessageContent(
        messageId: ChatMessageId,
        content: ChatMessage.Content,
        customContentDecoder: CustomContentDecoder,
    ) {
        updateMessagesContents(listOf(messageId to content), customContentDecoder)
    }

    override suspend fun updateMessagesContents(
        modifications: List<Pair<ChatMessageId, ChatMessage.Content>>,
        customContentDecoder: CustomContentDecoder
    ) = withContext(coroutineDispatchers.io) {
        val now = System.currentTimeMillis()
        val local = modifications.map { (id, content) ->
            val (contentSerialized, contentTypeLocal) = content.toLocal(customContentDecoder)
            MessageContentUpdateLocal(id, contentSerialized, contentTypeLocal, updatedAt = now)
        }
        chatMessageDao.updateMessagesContents(local)
    }

    override suspend fun updateMessagesStatus(
        messageIds: List<ChatMessageId>,
        status: ChatMessage.Status
    ) {
        if (messageIds.isEmpty()) return

        chatMessageDao.updateMessagesStatus(messageIds, status.toLocal(), updatedAt = System.currentTimeMillis())
    }

    override suspend fun updateOutgoingMessagesStatusForChat(
        chatId: ChatId,
        fromStatus: ChatMessage.Status,
        toStatus: ChatMessage.Status
    ) {
        chatMessageDao.updateOutgoingMessagesStatusForChat(chatId.toLocal(), fromStatus.toLocal(), toStatus.toLocal())
    }

    override suspend fun updateOutgoingMessagesStatusForChatWithTypes(
        chatId: ChatId,
        fromStatus: ChatMessage.Status,
        toStatus: ChatMessage.Status,
        contentTypes: List<KClass<out ChatMessage.Content>>
    ) {
        chatMessageDao.updateOutgoingMessagesStatusForChatWithTypes(chatId.toLocal(), fromStatus.toLocal(), toStatus.toLocal(), contentTypes.map { it.toLocalType() })
    }

    override suspend fun updateOutgoingMessagesStatusForChatExcludingTypes(
        chatId: ChatId,
        fromStatus: ChatMessage.Status,
        toStatus: ChatMessage.Status,
        contentTypes: List<KClass<out ChatMessage.Content>>
    ) {
        chatMessageDao.updateOutgoingMessagesStatusForChatExcludingTypes(chatId.toLocal(), fromStatus.toLocal(), toStatus.toLocal(), contentTypes.map { it.toLocalType() })
    }

    override suspend fun shiftChatMessagesToTimestamp(chatId: ChatId, timestamp: Timestamp) {
        chatMessageDao.updateTimestamps(chatId.toLocal(), timestamp)
    }

    override suspend fun removeMessage(messageId: ChatMessageId) {
        chatMessageDao.remove(messageId)
    }

    override suspend fun markMessagesAsReadUpToTimestamp(chatId: ChatId, timestamp: Timestamp) {
        chatMessageDao.markMessagesAsReadUpToTimestamp(chatId.toLocal(), timestamp)
    }

    override suspend fun markMessagesByTypesAsRead(chatId: ChatId, types: List<String>) {
        chatMessageDao.markMessagesByTypesAsRead(chatId.toLocal(), types)
    }

    override suspend fun markMessageAsRead(messageId: ChatMessageId) {
        chatMessageDao.markMessageAsRead(messageId)
    }

    override fun subscribeLastMessageSummaries(
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<LastMessageSummary>> {
        return chatRoomRepository.subscribeChatSummaries()
            .mapList { roomSummary ->
                LastMessageSummary(
                    chatId = ChatId.fromRawValue(roomSummary.chatId),
                    lastMessage = roomSummary.lastMessage?.toDomain(customContentDecoder),
                    lastMessageSortOrder = roomSummary.lastMessage?.sortOrder,
                    unseenCount = roomSummary.unseenCount,
                    hasUnseenReaction = roomSummary.hasUnseenReaction,
                    chatCreatedAt = roomSummary.createdAt,
                    roomMetadata = RoomMetadata(
                        name = roomSummary.roomName,
                        icon = roomSummary.roomIcon,
                    ),
                )
            }
    }

    override fun subscribeMessages(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<ChatMessage>> {
        return chatMessageDao
            .subscribeMessages(chatId.toLocal())
            .mapList { it.toDomain(customContentDecoder) }
    }

    override suspend fun getMessages(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder
    ): List<ChatMessage> {
        return chatMessageDao.getMessages(chatId.toLocal())
            .map { it.toDomain(customContentDecoder) }
    }

    override suspend fun getMessagesUpdatedAfter(
        after: Instant,
        customContentDecoder: CustomContentDecoder,
    ): List<ChatMessage> {
        return chatMessageDao.getMessagesUpdatedAfter(after.toEpochMilliseconds()).map { it.toDomain(customContentDecoder) }
    }

    override fun subscribeMessages(
        chatId: ChatId,
        direction: ChatMessageDirection,
        type: KClass<out ChatMessage.Content>,
        status: ChatMessage.Status,
        customContentDecoder: CustomContentDecoder
    ): Flow<List<ChatMessage>> {
        return chatMessageDao.subscribeMessages(
            chatId = chatId.toLocal(),
            type = type.toLocalType(),
            status = status.toLocal(),
            isOutgoing = direction == ChatMessageDirection.OUTGOING
        ).mapList { it.toDomain(customContentDecoder) }
    }

    override fun subscribeOutgoingMessagesByStatus(
        chatId: ChatId,
        status: ChatMessage.Status,
        customContentDecoder: CustomContentDecoder,
    ): Flow<List<ChatMessage>> {
        return chatMessageDao
            .subscribeOutgoingMessagesByStatus(chatId.toLocal(), status.toLocal())
            .mapList { it.toDomain(customContentDecoder) }
    }

    override fun subscribeMessageById(chatMessageId: ChatMessageId, customContentDecoder: CustomContentDecoder): Flow<ChatMessage?> {
        return chatMessageDao
            .subscribeMessageById(chatMessageId)
            .map { it?.toDomain(customContentDecoder) }
    }

    override fun subscribeCallSignalingMessages(customContentDecoder: CustomContentDecoder): Flow<List<ChatMessage>> {
        return chatMessageDao
            .subscribeMessagesByTypes(
                listOf(
                    ChatMessageLocal.Type.DATA_CHANNEL_ANSWER,
                    ChatMessageLocal.Type.DATA_CHANNEL_CLOSED
                )
            )
            .mapList { it.toDomain(customContentDecoder) }
    }

    override fun subscribeMessageStatus(chatMessageId: ChatMessageId): Flow<ChatMessage.Status?> {
        return chatMessageDao
            .subscribeMessageById(chatMessageId)
            .map { it?.status?.toDomain() }
    }

    override fun subscribeReactions(chatId: ChatId): Flow<List<ChatMessageReaction>> {
        return chatMessageReactionDao
            .subscribeReactions(chatId.toLocal())
            .mapList { it.toDomain() }
    }

    override suspend fun addReaction(reaction: ChatMessageReaction, chatId: ChatId) {
        chatMessageReactionDao.insertReaction(reaction.toLocal(chatId))
    }

    override suspend fun removeMessageReaction(reaction: ChatMessageReaction, chatId: ChatId) {
        chatMessageReactionDao.deleteReaction(reaction.toLocal(chatId))
    }

    override suspend fun getUnsupportedMessages(chatId: ChatId): List<ChatMessage> {
        val decoder = AlwaysFailCustomContentDecoder()
        return chatMessageDao
            .getUnsupportedMessages(chatId.toLocal())
            .map { it.toDomain(decoder) }
    }

    override suspend fun deleteAllChatMessages(chatId: ChatId) {
        chatMessageDao.deleteAllMessages(chatId.toLocal())
    }

    override suspend fun getMessageStatuses(messageIds: List<ChatMessageId>): Map<ChatMessageId, ChatMessage.Status> {
        if (messageIds.isEmpty()) return emptyMap()

        return chatMessageDao.getMessageStatuses(messageIds)
            .associate { it.id to it.status.toDomain() }
    }

    override suspend fun getMessageById(
        chatId: ChatId,
        messageId: ChatMessageId,
        customContentDecoder: CustomContentDecoder
    ): ChatMessage? {
        return chatMessageDao.getMessage(messageId)?.toDomain(customContentDecoder)
    }

    override suspend fun unwindUnackedCompactions(chatId: ChatId) {
        chatMessageCompactionDao.unwindUnackedCompactions(chatId.toLocal(), updatedAt = System.currentTimeMillis())
    }

    override suspend fun commitCompaction(commitId: ChatMessageId, originalIds: List<ChatMessageId>) {
        if (originalIds.isEmpty()) return

        chatMessageCompactionDao.commitCompaction(originalIds, commitId)
    }

    override suspend fun propagateStatusToCompactedMessages(commitId: ChatMessageId, status: ChatMessage.Status) {
        if (status != ChatMessage.Status.IS_SENT && status != ChatMessage.Status.IS_READ) return

        val upgradableFrom = ChatMessage.Status.entries.filter { it.deliveryRank < status.deliveryRank }

        chatMessageCompactionDao.propagateStatusToCompactionDescendants(
            commitId = commitId,
            fromStatuses = upgradableFrom.map { it.toLocal() },
            toStatus = status.toLocal(),
            updatedAt = System.currentTimeMillis()
        )
    }

    override suspend fun searchMessages(query: String, chatIds: List<ChatId>): Result<List<ChatMessageSearchHit>> {
        if (chatIds.isEmpty()) return Result.success(emptyList())

        return runCatching {
            val rawChatIds = chatIds.map { it.value.value }

            chatMessageDao.searchMessages(query.escapeLikeWildcards(), rawChatIds, MESSAGE_SEARCH_LIMIT).map { local ->
                ChatMessageSearchHit(
                    chatId = ChatId.fromRawValue(local.chatId),
                    messageId = local.id,
                    snippet = local.searchableContent,
                    timestamp = local.timestamp
                )
            }
        }
    }
}

private fun String.escapeLikeWildcards(): String {
    return replace("\\", "\\\\")
        .replace("%", "\\%")
        .replace("_", "\\_")
}

private const val MESSAGE_SEARCH_LIMIT = 50
