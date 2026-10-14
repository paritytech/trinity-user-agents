package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.FirstNewMessageInfo
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Immutable
data class ChatMessagesState(
    val messages: ImmutableList<ChatMessageUiModel> = persistentListOf(),
    val firstNewMessageInfo: FirstNewMessageInfo? = null,
    val unreadCounter: Int = 0,
    /** What the feed can load above the oldest message. */
    val olderPage: ChatPageEdgeUiState = ChatPageEdgeUiState.END,
    /** What the feed can load below the newest message. */
    val newerPage: ChatPageEdgeUiState = ChatPageEdgeUiState.END,
)

/** One end of the loaded messages of a room. A room that the host keeps is complete, so both ends are [END]. */
enum class ChatPageEdgeUiState {
    /** More messages exist past this end. The feed loads them when the user comes near. */
    MORE,
    LOADING,
    END,

    /** The last page did not load. The feed offers a retry. */
    FAILED,
}

val ChatPageEdgeUiState.canLoad: Boolean
    get() = this == ChatPageEdgeUiState.MORE || this == ChatPageEdgeUiState.FAILED

fun ChatPagingState.Edge.toUi(): ChatPageEdgeUiState = when (this) {
    ChatPagingState.Edge.MORE -> ChatPageEdgeUiState.MORE
    ChatPagingState.Edge.LOADING -> ChatPageEdgeUiState.LOADING
    ChatPagingState.Edge.END -> ChatPageEdgeUiState.END
    ChatPagingState.Edge.FAILED -> ChatPageEdgeUiState.FAILED
}
