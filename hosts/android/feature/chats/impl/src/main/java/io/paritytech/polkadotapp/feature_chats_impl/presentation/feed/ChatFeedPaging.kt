package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatPageEdgeUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.canLoad
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first

/** The paging and the jumps of the room screen. A room that the host keeps is complete, so it asks nothing. */
internal class ChatFeedPaging(
    private val chatId: ChatId,
    private val interactor: ChatFeedInteractor,
    private val messagesState: StateFlow<LoadingState<ChatMessagesState>>,
) {
    private val current: ChatMessagesState?
        get() = messagesState.value.dataOrNull

    suspend fun loadOlder() {
        if (current?.olderPage?.canLoad == true) interactor.loadOlderMessages(chatId)
    }

    suspend fun loadNewer() {
        if (current?.newerPage?.canLoad == true) interactor.loadNewerMessages(chatId)
    }

    suspend fun showNewest() {
        if (current?.newerPage != ChatPageEdgeUiState.END) interactor.showNewestMessages(chatId)
    }

    /** Pages a product-source room until the message is loaded, then waits until the feed shows it. */
    suspend fun awaitLoaded(messageId: ChatMessageId) {
        if (!isLoaded(current, messageId)) interactor.jumpToMessage(chatId, messageId)

        messagesState.first { isLoaded(it.dataOrNull, messageId) }
    }

    private fun isLoaded(state: ChatMessagesState?, messageId: ChatMessageId) = state?.messages.orEmpty().any { it.id == messageId }
}
