package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.components

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchNoResults
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchPersonRow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.NoRowStatus
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.RecentChatUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.AddContactSearchResults
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.AddContactUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.UserSearchResultUiModel
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun AddContactSearchContent(
    state: AddContactUiState,
    onSearchResultClick: (UserSearchResultUiModel) -> Unit,
    onRecentClick: (ChatId) -> Unit,
) {
    when (val results = state.results) {
        AddContactSearchResults.Idle -> if (state.recents.isEmpty()) {
            CenteredMessage(text = AnnotatedString(stringResource(RCommon.string.add_contact_no_recent_searches)))
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                recentsSection(recents = state.recents, onRecentClick = onRecentClick)
            }
        }

        is AddContactSearchResults.Sections -> LazyColumn(modifier = Modifier.fillMaxSize()) {
            recentsSection(recents = results.recents, onRecentClick = onRecentClick)
            allUsersSection(
                users = results.allUsers,
                loadingContactId = state.loadingContactId,
                onClick = onSearchResultClick,
            )
        }

        AddContactSearchResults.Waiting -> Unit

        AddContactSearchResults.Loading -> CenteredLoading()

        AddContactSearchResults.Empty -> ChatSearchNoResults(query = state.searchQuery)

        AddContactSearchResults.Error -> CenteredMessage(
            text = AnnotatedString(stringResource(RCommon.string.add_contact_search_error))
        )
    }
}

private fun LazyListScope.recentsSection(
    recents: ImmutableList<RecentChatUiModel>,
    onRecentClick: (ChatId) -> Unit,
) {
    if (recents.isEmpty()) return

    sectionHeader(key = "recent_header", titleRes = RCommon.string.search_section_recent)
    items(
        items = recents,
        key = { recent -> "recent_${recent.key}" }
    ) { recent ->
        ChatSearchPersonRow(
            title = recent.title,
            avatarModel = recent.avatarModel,
            status = recent.status,
            onClick = { onRecentClick(recent.chatId) },
        )
    }
}

private fun LazyListScope.allUsersSection(
    users: ImmutableList<UserSearchResultUiModel>,
    loadingContactId: AccountId?,
    onClick: (UserSearchResultUiModel) -> Unit,
) {
    if (users.isEmpty()) return

    sectionHeader(key = "user_header", titleRes = RCommon.string.search_section_all_users)
    items(
        items = users,
        key = { user -> "user_${user.contactAccountId.value.toHexString()}" }
    ) { user ->
        ChatSearchPersonRow(
            title = user.username,
            avatarModel = user.avatarModel,
            status = NoRowStatus,
            onClick = { onClick(user) },
            loading = user.contactAccountId == loadingContactId,
        )
    }
}

private fun LazyListScope.sectionHeader(key: String, @StringRes titleRes: Int) {
    item(key = key) {
        NovaText(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.extraMedium,
                vertical = PolkadotTheme.spacings.small
            ),
            text = stringResource(titleRes),
            style = PolkadotTheme.typography.caption.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun CenteredMessage(text: AnnotatedString) {
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.Center
    ) {
        NovaText(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.large),
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )
    }
}

@Composable
private fun CenteredLoading() {
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.Center
    ) {
        NovaCircularProgressIndicator(
            modifier = Modifier.size(32.dp),
            color = PolkadotTheme.colors.fg.primary,
            strokeWidth = 3.dp
        )
    }
}
