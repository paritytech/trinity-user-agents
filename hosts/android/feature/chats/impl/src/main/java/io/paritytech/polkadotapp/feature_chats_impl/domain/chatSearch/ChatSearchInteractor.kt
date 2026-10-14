package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatMessageSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.RecentChat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData
import io.paritytech.polkadotapp.feature_chats_impl.domain.search.CompoundChatSearchResultProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SubscribeActiveChatsUseCase
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

interface ChatSearchInteractor {
    context(scope: ComputationalScope)
    fun subscribeChats(): Flow<List<Chat>>

    suspend fun search(query: String, chats: List<Chat>): Result<List<ChatListSearchResult>>

    suspend fun onAppResultSelected(result: ChatListSearchResult.App)

    suspend fun resolveStartChatData(accountId: AccountId): Result<StartChatData>

    fun observeRecents(): Flow<List<RecentChat>>

    suspend fun addRecent(chatId: ChatId)

    suspend fun removeRecent(chatId: ChatId)

    suspend fun clearRecents()
}

class RealChatSearchInteractor @Inject constructor(
    private val subscribeActiveChats: SubscribeActiveChatsUseCase,
    private val chatMessageRepository: ChatMessageRepository,
    private val compoundChatSearchResultProvider: CompoundChatSearchResultProvider,
    private val startChatDataUseCase: StartChatDataUseCase,
    private val chatSearchRecentsRepository: ChatSearchRecentsRepository,
) : ChatSearchInteractor {
    context(scope: ComputationalScope)
    override fun subscribeChats(): Flow<List<Chat>> = subscribeActiveChats()

    override suspend fun search(query: String, chats: List<Chat>): Result<List<ChatListSearchResult>> = coroutineScope {
        val messagesDeferred = async { searchMessages(query, chats.map { it.id }) }
        val appsDeferred = async { searchApps(query) }

        val messageHits = messagesDeferred.await().logFailure("ChatSearch.messages")
        val apps = appsDeferred.await()

        messageHits.map { hits -> mergeChatSearchResults(query, chats, hits, apps) }
    }

    override suspend fun onAppResultSelected(result: ChatListSearchResult.App) {
        compoundChatSearchResultProvider.onResultSelected(result)
    }

    override suspend fun resolveStartChatData(accountId: AccountId): Result<StartChatData> {
        return startChatDataUseCase(accountId)
    }

    override fun observeRecents(): Flow<List<RecentChat>> {
        return chatSearchRecentsRepository.observeRecents()
    }

    override suspend fun addRecent(chatId: ChatId) {
        chatSearchRecentsRepository.addRecent(chatId)
    }

    override suspend fun removeRecent(chatId: ChatId) {
        chatSearchRecentsRepository.removeRecent(chatId)
    }

    override suspend fun clearRecents() {
        chatSearchRecentsRepository.clearRecents()
    }

    private suspend fun searchMessages(query: String, chatIds: List<ChatId>): Result<List<ChatMessageSearchHit>> {
        return chatMessageRepository.searchMessages(query, chatIds)
    }

    private suspend fun searchApps(query: String): List<ChatListSearchResult.App> {
        return compoundChatSearchResultProvider.search(query)
    }
}

internal fun mergeChatSearchResults(
    query: String,
    chats: List<Chat>,
    messageHits: List<ChatMessageSearchHit>,
    apps: List<ChatListSearchResult.App>,
): List<ChatListSearchResult> {
    val existingChats = chats.filter { chat ->
        chat.display.name.contains(query, ignoreCase = true)
    }.map { chat ->
        ChatListSearchResult.Chat(
            id = chat.id.value.value.toHexString(),
            title = chat.display.name,
            chatId = chat.id
        )
    }

    val chatTitleByChatId = chats.associate { it.id to it.display.name }
    // A hit whose chat is not in the visible list has no displayable title — drop it instead of
    // leaking the snippet into the title position (double-snippet row).
    val messages = messageHits.mapNotNull { hit ->
        val chatTitle = chatTitleByChatId[hit.chatId] ?: return@mapNotNull null
        ChatListSearchResult.Message(
            id = hit.messageId,
            title = chatTitle,
            chatId = hit.chatId,
            messageId = hit.messageId,
            snippet = hit.snippet,
            timestamp = hit.timestamp
        )
    }

    return existingChats + messages + apps
}
