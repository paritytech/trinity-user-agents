package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine

/**
 * The messages of a room that the host database keeps: a person chat or a host-store product room. The
 * window is the whole room. The writes of the user keep the paths of [ChatEngine], so [perform] refuses them.
 */
class HostStoreMessageSource(
    private val chatId: ChatId,
    private val chatEngine: ChatEngine,
) : ChatMessageSource {
    override fun observeSnapshots(): Flow<ChatFeedSnapshot> = combine(
        chatEngine.subscribeMessagesFeed(chatId),
        chatEngine.subscribeReactions(chatId),
        chatEngine.subscribeRevisions(chatId),
    ) { messages, reactions, revisions ->
        ChatFeedSnapshot(
            messages = messages,
            reactions = reactions,
            revisions = revisions,
            paging = ChatPagingState.Complete,
            readiness = ChatRoomReadiness.READY,
        )
    }

    override suspend fun loadOlder(): ChatPageOutcome = ChatPageOutcome.End

    override suspend fun loadNewer(): ChatPageOutcome = ChatPageOutcome.End

    override suspend fun jump(messageId: ChatMessageId): Boolean = false

    override suspend fun perform(action: ChatSourceAction): ChatSourceActionOutcome = ChatSourceActionOutcome.Refused(reason = null)

    override suspend fun reload() = Unit

    override suspend fun markUnavailable() = Unit

    override fun close() = Unit
}
