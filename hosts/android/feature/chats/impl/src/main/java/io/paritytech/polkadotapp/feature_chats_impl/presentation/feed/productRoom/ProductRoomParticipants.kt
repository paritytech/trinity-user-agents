package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isUser
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.participants.ChatParticipantDirectory
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What the room screen shows of the participants of a product room (P6.3). */
@Immutable
data class ProductParticipantsUiState(
    /** The room asks its product for its participants, so the toolbar offers the list. */
    val declared: Boolean,
    /** The count for the header, or null to keep the description. */
    val count: Int?,
    /** The author of each message from a participant other than the user and the product. */
    val authors: ImmutableMap<ChatMessageId, ChatParticipantUiModel>,
    val list: ParticipantListUiState,
) {
    companion object {
        val None = ProductParticipantsUiState(declared = false, count = null, authors = persistentMapOf(), list = ParticipantListUiState.Empty)
    }
}

@Immutable
data class ParticipantListUiState(
    val participants: ImmutableList<ChatParticipantUiModel>,
    val loading: Boolean,
    val failed: Boolean,
    val hasMore: Boolean,
) {
    companion object {
        val Empty = ParticipantListUiState(participants = persistentListOf(), loading = false, failed = false, hasMore = true)
    }
}

/**
 * The participants of a product room (02-DESIGN §3.3.7, scenario S30): the count in the header, the author of each
 * message, and the list, paged through `participants_query`. A room that does not declare its participants has two,
 * the user and the product, and offers no list.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomParticipants(
    private val scope: CoroutineScope,
    chatId: ChatId,
    roomPolicy: Flow<ChatRoomPolicy?>,
    feedSnapshot: Flow<ChatFeedSnapshot>,
    private val origins: Flow<MessageOriginDisplayResolver>,
    private val appearances: ChatParticipantAppearanceFactory,
    querying: suspend () -> ProductChatQuerying?,
) {
    private data class Declaration(val declared: Boolean, val generation: Long)

    private data class Paging(
        val entries: List<ChatParticipantEntryModel>,
        val next: String?,
        val ended: Boolean,
        val loading: Boolean,
        val failed: Boolean,
    )

    val directory = ChatParticipantDirectory(chatId, scope, querying)
    private val productOrigin = ChatMessageOrigin.Extension(chatId.extensionOrNull()?.extensionId.orEmpty())

    private val paging = MutableStateFlow(InitialPaging)
    private var pageJob: Job? = null

    private val declaration = roomPolicy
        .map { Declaration(it?.declaresParticipants() == true, it?.configGeneration ?: 0) }
        .distinctUntilChanged()
        .onEach(::restart)
        .stateIn(scope, SharingStarted.Eagerly, Declaration(declared = false, generation = 0))

    private val authors = combine(declaration, feedSnapshot, directory.resolved, origins) { declaration, snapshot, resolved, origins ->
        if (!declaration.declared) return@combine emptyMap()

        val authorIds = snapshot.messages
            .filterNot { it.origin.isUser() }
            .mapNotNull { message -> message.originParticipantId?.takeUnless { it in RESERVED }?.let { message.id to it } }
            .toMap()
        directory.resolve(authorIds.values)

        val byParticipant = authorIds.values.toSet().associateWith { appearance(it, resolved[it], origins) }
        authorIds.mapValues { (_, participantId) -> byParticipant.getValue(participantId) }
    }

    private val list = combine(paging, directory.resolved, origins) { paging, _, origins ->
        ParticipantListUiState(
            participants = paging.entries.map { appearance(it.participantId, it, origins) }.toImmutableList(),
            loading = paging.loading,
            failed = paging.failed,
            hasMore = !paging.ended,
        )
    }

    val uiState: StateFlow<ProductParticipantsUiState> = combine(declaration, directory.count, authors, list) { declaration, count, authors, list ->
        ProductParticipantsUiState(
            declared = declaration.declared,
            count = count.takeIf { declaration.declared },
            authors = authors.toImmutableMap(),
            list = list,
        )
    }.stateIn(scope, SharingStarted.Eagerly, ProductParticipantsUiState.None)

    /** The next page of the list. The first call asks for the first page. */
    fun loadMore() {
        val current = paging.value
        if (!declaration.value.declared || current.loading || current.ended) return

        paging.update { it.copy(loading = true, failed = false) }
        pageJob = scope.launch {
            directory.page(current.next)
                .onSuccess { page ->
                    paging.update { it.copy(entries = it.entries + page.participants, next = page.next, ended = page.next == null, loading = false) }
                }
                .onFailure { paging.update { it.copy(loading = false, failed = true) } }
        }
    }

    /** The appearance of one participant, after the product resolved it when it can. */
    suspend fun appearanceOf(participantId: String): ChatParticipantUiModel {
        directory.resolveNow(listOf(participantId))
        return appearance(participantId, directory.resolved.value[participantId], origins.first())
    }

    fun entryOf(participantId: String): ChatParticipantEntryModel? = directory.resolved.value[participantId]

    private suspend fun appearance(participantId: String, entry: ChatParticipantEntryModel?, origins: MessageOriginDisplayResolver) =
        appearances.appearance(participantId, entry, origins, productOrigin)

    private fun restart(declaration: Declaration) {
        pageJob?.cancel()
        directory.reset()
        paging.value = InitialPaging
        if (declaration.declared) directory.loadCount()
    }

    private companion object {
        val InitialPaging = Paging(entries = emptyList(), next = null, ended = false, loading = false, failed = false)
        val RESERVED = setOf(ProductParticipant.USER, ProductParticipant.PRODUCT)
    }
}

private fun ChatRoomPolicy.declaresParticipants() = ChatRoomPolicy.ProductQuery.PARTICIPANTS in productQueries
