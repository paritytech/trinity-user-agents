package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.search.SearchState
import io.paritytech.polkadotapp.common.presentation.search.withSearching
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_chats_api.domain.error.asStartChatError
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.presentation.error.toPresentationError
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.ChatSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchRowAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.toSectionsState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.toChatFeedPayload
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.map
import javax.inject.Inject

@HiltViewModel
class ChatSearchViewModel @Inject constructor(
    private val interactor: ChatSearchInteractor,
    private val router: ChatsRouter,
) : BaseViewModel() {
    private val searchQuery = MutableStateFlow("")
    private val openMenuForChatId = MutableStateFlow<ChatId?>(null)

    private val chats: StateFlow<List<Chat>> = interactor.subscribeChats()
        .stateInBackground(initialValue = emptyList())

    private val searchResults = combineToPair(searchQuery, chats)
        // The chat list re-emits on every incoming message; restarting the search each time would
        // flash the loading state, so only a new query or a changed searchable chat set re-runs it.
        .distinctUntilChangedBy { (query, chats) -> query to chats.map { it.id to it.display.name } }
        .map { (query, chats) -> (query to chats).takeIf { query.isNotEmpty() } }
        .withSearching { (query, chats) -> interactor.search(query, chats) }

    private val sections = combine(
        searchResults,
        chats,
        searchQuery
    ) { results, currentChats, query ->
        results.toSectionsState(query, currentChats.associateBy { it.id })
    }
        .inBackground()

    private val recents = combine(
        interactor.observeRecents(),
        openMenuForChatId,
        chats
    ) { recentChats, menuChatId, currentChats ->
        val chatsById = currentChats.associateBy { it.id }

        recentChats.mapNotNull { recent ->
            recent.toUi(isMenuOpen = recent.chatId == menuChatId, chatsById = chatsById)
        }.toImmutableList()
    }

    val state: StateFlow<ChatSearchUiState> = combine(
        searchQuery,
        sections,
        recents
    ) { query, sectionsState, recentsUi ->
        ChatSearchUiState(
            query = query,
            results = sectionsState,
            recents = recentsUi
        )
    }
        .stateInBackground(initialValue = InitialSearchUiState)

    fun onQueryChanged(value: String) {
        searchQuery.value = value
    }

    fun onBackClick() {
        router.back()
    }

    fun onResultTapped(action: ChatSearchRowAction) = launchUnit {
        when (action) {
            is ChatSearchRowAction.OpenChat -> onChatResultTapped(action.chatId)
            is ChatSearchRowAction.OpenMessage -> onMessageResultTapped(action.chatId, action.messageId)
            is ChatSearchRowAction.OpenApp -> interactor.onAppResultSelected(action.app)
        }
    }

    // A Recent's chatId may still be an un-established Contact (a person searched and tapped
    // once, but not yet a persisted chat), so re-resolve via StartChatDataUseCase rather than
    // assuming ExistingChat.
    fun onRecentTapped(chatId: ChatId) = launchUnit {
        when (val variant = chatId.chatVariant()) {
            is ChatVariant.Contact -> {
                interactor.resolveStartChatData(variant.contactAccountId)
                    .onSuccess { startChatData ->
                        router.openChatFeed(startChatData.toChatFeedPayload())
                    }
                    .onFailure { showPresentationError(it.asStartChatError().toPresentationError()) }
            }

            is ChatVariant.Extension -> {
                router.openChatFeed(ChatFeedPayload.existingChat(chatId))
            }
        }
    }

    fun onRecentLongPress(chatId: ChatId) {
        openMenuForChatId.value = chatId
    }

    fun onDismissRecentMenu() {
        openMenuForChatId.value = null
    }

    fun onRemoveRecent(chatId: ChatId) = launchUnit {
        interactor.removeRecent(chatId)
        onDismissRecentMenu()
    }

    fun onClearRecents() = launchUnit {
        interactor.clearRecents()
    }

    private suspend fun onChatResultTapped(chatId: ChatId) {
        interactor.addRecent(chatId)
        router.openChatFeed(ChatFeedPayload.existingChat(chatId))
    }

    private fun onMessageResultTapped(chatId: ChatId, messageId: ChatMessageId) {
        router.openChatFeed(ChatFeedPayload.existingChat(chatId, targetMessageId = messageId))
    }
}

private val InitialSearchUiState = ChatSearchUiState(
    query = "",
    results = SearchState.Initial,
    recents = persistentListOf()
)
