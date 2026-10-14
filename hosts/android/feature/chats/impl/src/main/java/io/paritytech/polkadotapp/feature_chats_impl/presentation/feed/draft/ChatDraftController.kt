package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.computeChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.isEmpty
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatSendMessageInputState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.InputMessageRelation
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toDomainDraft
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.DelicateCoroutinesApi
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlin.time.Duration.Companion.milliseconds

private val DRAFT_PERSIST_DEBOUNCE = 300.milliseconds

class ChatDraftController @AssistedInject constructor(
    @Assisted private val scope: CoroutineScope,
    @Assisted private val openChatRequest: OpenChatRequest,
    @Assisted private val input: MutableStateFlow<ChatSendMessageInputState>,
    private val chatDraftRepository: ChatDraftRepository,
    private val chatEngine: ChatEngine,
    private val messageUiMapper: ChatMessageUiMapper,
    private val dispatchers: CoroutineDispatchers
) {
    private val chatId = openChatRequest.computeChatId()

    fun start() {
        restore()
        persistOnChange()
    }

    @OptIn(DelicateCoroutinesApi::class)
    fun flush() {
        val draft = input.value.toDomainDraft()
        GlobalScope.launch(dispatchers.io) { persist(draft) }
    }

    private fun restore() = scope.launchUnit {
        val restored = chatDraftRepository.getDraft(chatId)?.toInputState() ?: return@launchUnit
        input.update { current ->
            if (current == ChatSendMessageInputState()) restored else current
        }
    }

    private fun persistOnChange() {
        input
            .drop(1)
            .map { it.toDomainDraft() }
            .distinctUntilChanged()
            .debounce(DRAFT_PERSIST_DEBOUNCE)
            .onEach { persist(it) }
            .launchIn(scope)
    }

    private suspend fun persist(draft: ChatDraft) {
        if (draft.isEmpty()) {
            chatDraftRepository.deleteDraft(chatId)
        } else {
            chatDraftRepository.saveDraft(chatId, draft)
        }
    }

    private suspend fun ChatDraft.toInputState(): ChatSendMessageInputState {
        val inputRelation = when (val relation = relation) {
            ChatDraftRelation.None -> InputMessageRelation.None
            is ChatDraftRelation.Edit -> rebuildEdit(relation.messageId, relation.originalText)
            is ChatDraftRelation.Reply -> rebuildReply(relation.messageId)
        }
        return ChatSendMessageInputState(inputMessage = text, relation = inputRelation)
    }

    private suspend fun rebuildEdit(messageId: ChatMessageId, originalText: String): InputMessageRelation {
        chatEngine.getMessageById(chatId, messageId) ?: return InputMessageRelation.None
        return InputMessageRelation.Edit(messageId, originalText)
    }

    private suspend fun rebuildReply(messageId: ChatMessageId): InputMessageRelation {
        val message = chatEngine.getMessageById(chatId, messageId) ?: return InputMessageRelation.None
        return messageUiMapper.createReplyRelationFor(message, chatEngine.originDisplayResolver(chatId))
            ?: InputMessageRelation.None
    }

    @AssistedFactory
    interface Factory {
        fun create(
            scope: CoroutineScope,
            openChatRequest: OpenChatRequest,
            input: MutableStateFlow<ChatSendMessageInputState>
        ): ChatDraftController
    }
}
