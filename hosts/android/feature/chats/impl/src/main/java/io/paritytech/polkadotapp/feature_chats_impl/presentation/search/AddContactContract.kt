package io.paritytech.polkadotapp.feature_chats_impl.presentation.search

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.RecentChatUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.UserSearchResultUiModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

@Immutable
data class AddContactUiState(
    val searchQuery: String,
    val results: AddContactSearchResults,
    val loadingContactId: AccountId?,
    val recents: ImmutableList<RecentChatUiModel>,
)

@Immutable
sealed interface AddContactSearchResults {
    data object Idle : AddContactSearchResults

    data class Sections(
        val recents: ImmutableList<RecentChatUiModel>,
        val allUsers: ImmutableList<UserSearchResultUiModel>,
    ) : AddContactSearchResults

    data object Waiting : AddContactSearchResults

    data object Loading : AddContactSearchResults

    data object Empty : AddContactSearchResults

    data object Error : AddContactSearchResults
}

interface AddContactContract {
    val state: StateFlow<AddContactUiState>

    fun onSearchChange(value: String)
    fun onSearchResultClick(result: UserSearchResultUiModel)
    fun onRecentClick(chatId: ChatId)
}
