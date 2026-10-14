package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SubscribeActiveChatsUseCase
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

interface ChatListInteractor {
    context(scope: ComputationalScope)
    fun subscribeChats(): Flow<List<Chat>>

    context(scope: ComputationalScope)
    fun subscribeCallSignaling(): Flow<List<ChatMessage>>

    fun subscribeActiveCall(): Flow<ActiveCallState?>

    fun subscribeAllCustomMessageRenderers(): Flow<CustomChatMessageRenderersById>

    fun subscribePendingIncomingRequestsCount(): Flow<Int>

    suspend fun handleChatOpen(chatId: ChatId): Boolean

    fun subscribeDrafts(): Flow<Map<ChatId, ChatDraft>>
}

class RealChatListInteractor @Inject constructor(
    private val chatEngine: ChatEngine,
    private val contactsRepository: ContactsRepository,
    private val callStateTracker: CallStateTracker,
    private val chatDraftRepository: ChatDraftRepository,
    private val subscribeActiveChats: SubscribeActiveChatsUseCase,
) : ChatListInteractor {
    context(scope: ComputationalScope)
    override fun subscribeChats(): Flow<List<Chat>> = subscribeActiveChats()

    context(scope: ComputationalScope)
    override fun subscribeCallSignaling(): Flow<List<ChatMessage>> = chatEngine.subscribeCallSignaling()

    override fun subscribeActiveCall(): Flow<ActiveCallState?> = callStateTracker.observeActiveCall()

    override fun subscribeAllCustomMessageRenderers() = chatEngine.observeActiveCustomMessageRenderers()

    override fun subscribePendingIncomingRequestsCount() = contactsRepository.subscribePendingIncomingRequestsCount()

    override suspend fun handleChatOpen(chatId: ChatId): Boolean =
        chatEngine.getOpenHandlerForChat(chatId)?.handleOpen() ?: false

    override fun subscribeDrafts(): Flow<Map<ChatId, ChatDraft>> = chatDraftRepository.subscribeDrafts()
}
