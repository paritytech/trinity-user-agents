package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models

import androidx.annotation.StringRes
import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.search.SearchState
import io.paritytech.polkadotapp.common.utils.SizedList
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class ChatSearchUiState(
    val query: String,
    val results: SearchState<SizedList<ChatSearchSectionUiModel>>,
    val recents: ImmutableList<RecentChatUiModel>,
)

@Immutable
data class ChatSearchSectionUiModel(
    val key: String,
    @StringRes val titleRes: Int,
    val rows: ImmutableList<ChatSearchRowUiModel>,
)

@Immutable
data class ChatSearchRowStatus(
    val isMuted: Boolean,
    val hasReaction: Boolean,
    val badge: ChatListUiState.Badge,
)

@Immutable
sealed interface ChatSearchRowUiModel {
    val key: String
    val title: String
    val avatarModel: AvatarUiModel
    val status: ChatSearchRowStatus
    val action: ChatSearchRowAction

    data class Person(
        override val key: String,
        override val title: String,
        override val avatarModel: AvatarUiModel,
        override val status: ChatSearchRowStatus,
        override val action: ChatSearchRowAction,
    ) : ChatSearchRowUiModel

    data class Message(
        override val key: String,
        override val title: String,
        override val avatarModel: AvatarUiModel,
        override val status: ChatSearchRowStatus,
        override val action: ChatSearchRowAction,
        val timestamp: Timestamp,
        val snippet: String,
        val snippetHighlights: ImmutableList<IntRange>,
    ) : ChatSearchRowUiModel
}

@Immutable
sealed interface ChatSearchRowAction {
    data class OpenChat(val chatId: ChatId) : ChatSearchRowAction

    data class OpenMessage(val chatId: ChatId, val messageId: ChatMessageId) : ChatSearchRowAction

    data class OpenApp(val app: ChatListSearchResult.App) : ChatSearchRowAction
}

@Immutable
data class RecentChatUiModel(
    val chatId: ChatId,
    val key: String,
    val title: String,
    val avatarModel: AvatarUiModel,
    val status: ChatSearchRowStatus,
    val isMenuOpen: Boolean,
)
