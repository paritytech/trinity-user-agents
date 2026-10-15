package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.ChatRoomSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchAuthor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchCursor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.foldQuery
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.offersRoomSearch
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.SearchExcerpt
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.buildFoldedSearchExcerpt
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds

/** What the room screen shows of the in-room search (07-LATER §8.2). */
@Immutable
data class ChatRoomSearchUiState(
    /** The header shows the search action. */
    val isAvailable: Boolean = false,
    /** The search field replaces the header. */
    val isActive: Boolean = false,
    val query: String = "",
    val results: Results = Results.Idle,
) {
    sealed interface Results {
        /** No text to search yet. */
        data object Idle : Results

        data object Loading : Results

        /** Newest first. */
        data class Hits(
            val items: ImmutableList<RoomSearchResultUiModel>,
            val canLoadMore: Boolean,
            val isLoadingMore: Boolean,
        ) : Results

        data object NoResults : Results

        /** The product did not answer, or does not search: "Search is not available". */
        data object NotAvailable : Results

        data object Failed : Results
    }
}

/** One result: the author, the time and the line that matches, with the match marked. */
@Immutable
data class RoomSearchResultUiModel(
    val messageId: ChatMessageId,
    val author: Author,
    val timestamp: Long,
    val excerpt: SearchExcerpt,
) {
    sealed interface Author {
        data object You : Author

        data class Named(val name: String) : Author
    }
}

sealed interface RoomSearchAction {
    data object Open : RoomSearchAction

    data object Close : RoomSearchAction

    data class QueryChanged(val text: String) : RoomSearchAction

    /** The user came near the oldest result. */
    data object LoadMore : RoomSearchAction

    /** The user tapped a result: the field closes and the room jumps to the message. */
    data class Select(val messageId: ChatMessageId) : RoomSearchAction
}

/**
 * The in-room search of one room screen. The host searches a room that it keeps. In a room whose product keeps the
 * messages, the product answers `search_query` after 300 ms without typing; the host never asks it from the chat
 * list (D29).
 */
@OptIn(FlowPreview::class)
class ChatRoomSearchController(
    private val scope: CoroutineScope,
    private val chatId: ChatId,
    private val roomPolicy: Flow<ChatRoomPolicy?>,
    private val roomName: Flow<String?>,
    private val interactor: ChatRoomSearchInteractor,
    private val servedFeatures: Collection<String>,
    private val onJump: (ChatMessageId) -> Unit,
) {
    private val isActive = MutableStateFlow(false)
    private val query = MutableStateFlow("")
    private val progress = MutableStateFlow<Progress>(Progress.Idle)
    private var loadMoreJob: Job? = null

    val uiState: StateFlow<ChatRoomSearchUiState> = combine(
        combine(roomPolicy, isActive, query, ::Triple),
        roomName,
        progress,
    ) { (policy, active, text), name, current ->
        val available = policy.offersRoomSearch(servedFeatures)
        ChatRoomSearchUiState(
            isAvailable = available,
            isActive = available && active,
            query = text,
            results = current.toResults(name.orEmpty()),
        )
    }.stateIn(scope, SharingStarted.Eagerly, ChatRoomSearchUiState())

    init {
        scope.launch {
            combine(isActive, query) { active, text -> if (active) text.trim() else "" }
                .debounce(TYPING_PAUSE)
                .distinctUntilChanged()
                .collectLatest(::searchFirstPage)
        }
    }

    fun handle(action: RoomSearchAction) {
        when (action) {
            RoomSearchAction.Open -> isActive.value = true
            RoomSearchAction.Close -> close()
            is RoomSearchAction.QueryChanged -> query.value = action.text.take(MAX_QUERY_LENGTH)
            RoomSearchAction.LoadMore -> loadMore()
            is RoomSearchAction.Select -> {
                close()
                onJump(action.messageId)
            }
        }
    }

    private fun close() {
        isActive.value = false
        query.value = ""
        loadMoreJob?.cancel()
        progress.value = Progress.Idle
    }

    private suspend fun searchFirstPage(text: String) {
        loadMoreJob?.cancel()
        if (text.isEmpty()) {
            progress.value = Progress.Idle
            return
        }
        val policy = roomPolicy.first() ?: return

        progress.value = Progress.Loading(text)
        progress.value = interactor.search(chatId, policy, text, cursor = null).fold(
            onSuccess = { page -> page.toProgress(text, earlier = emptyList()) },
            onFailure = { Progress.Failed(text) },
        )
    }

    private fun loadMore() {
        val loaded = progress.value as? Progress.Loaded ?: return
        val cursor = loaded.next ?: return
        if (loaded.isLoadingMore) return

        progress.value = loaded.copy(isLoadingMore = true)
        loadMoreJob = scope.launch {
            val policy = roomPolicy.first() ?: return@launch
            val outcome = interactor.search(chatId, policy, loaded.text, cursor)
            progress.update { current ->
                if (current !is Progress.Loaded || current.text != loaded.text) return@update current
                // A later page that fails keeps the hits that the screen shows.
                outcome.getOrNull()?.toProgress(loaded.text, earlier = loaded.hits) ?: loaded.copy(isLoadingMore = false, next = null)
            }
        }
    }

    private fun RoomSearchPage.toProgress(text: String, earlier: List<RoomSearchHit>): Progress = when (this) {
        RoomSearchPage.NotAvailable -> if (earlier.isEmpty()) Progress.NotAvailable(text) else Progress.Loaded(text, earlier, null, false)
        is RoomSearchPage.Hits -> Progress.Loaded(text, earlier + hits, next, isLoadingMore = false)
    }

    private sealed interface Progress {
        data object Idle : Progress

        data class Loading(val text: String) : Progress

        data class Loaded(val text: String, val hits: List<RoomSearchHit>, val next: RoomSearchCursor?, val isLoadingMore: Boolean) : Progress

        data class NotAvailable(val text: String) : Progress

        data class Failed(val text: String) : Progress
    }

    private fun Progress.toResults(roomName: String): ChatRoomSearchUiState.Results = when (this) {
        Progress.Idle -> ChatRoomSearchUiState.Results.Idle
        is Progress.Loading -> ChatRoomSearchUiState.Results.Loading
        is Progress.NotAvailable -> ChatRoomSearchUiState.Results.NotAvailable
        is Progress.Failed -> ChatRoomSearchUiState.Results.Failed
        is Progress.Loaded -> if (hits.isEmpty()) {
            ChatRoomSearchUiState.Results.NoResults
        } else {
            val foldedQuery = foldQuery(text)
            ChatRoomSearchUiState.Results.Hits(
                items = hits.map { it.toUi(foldedQuery, roomName) }.toImmutableList(),
                canLoadMore = next != null,
                isLoadingMore = isLoadingMore,
            )
        }
    }

    private fun RoomSearchHit.toUi(foldedQuery: String, roomName: String) = RoomSearchResultUiModel(
        messageId = messageId,
        author = when (val author = author) {
            RoomSearchAuthor.User -> RoomSearchResultUiModel.Author.You
            RoomSearchAuthor.Room -> RoomSearchResultUiModel.Author.Named(roomName)
            is RoomSearchAuthor.Participant -> RoomSearchResultUiModel.Author.Named(author.participantId)
        },
        timestamp = timestamp,
        excerpt = buildFoldedSearchExcerpt(text, foldedQuery),
    )

    class Factory @Inject constructor(
        private val interactor: ChatRoomSearchInteractor,
    ) {
        fun create(
            scope: CoroutineScope,
            chatId: ChatId,
            roomPolicy: Flow<ChatRoomPolicy?>,
            roomName: Flow<String?>,
            onJump: (ChatMessageId) -> Unit,
            servedFeatures: Collection<String> = ChatHostCapabilities.features,
        ) = ChatRoomSearchController(scope, chatId, roomPolicy, roomName, interactor, servedFeatures, onJump)
    }

    private companion object {
        /** The host asks after 300 ms without typing (07-LATER §8.2). */
        val TYPING_PAUSE = 300.milliseconds

        /** The core takes at most 256 bytes of search text (07-LATER §3.5). */
        const val MAX_QUERY_LENGTH = 64
    }
}
