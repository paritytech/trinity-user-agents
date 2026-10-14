package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.search.SearchState
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotSearchAppBar
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.ChatSearchViewModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchNoResults
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchPersonAvatarSize
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchPersonRow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchPrompt
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.ChatSearchResultsSections
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components.RecentActionMenu
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchRowAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.RecentChatUiModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ChatSearchScreen() {
    val viewModel = hiltViewModel<ChatSearchViewModel>()
    val state by viewModel.state.collectAsStateWithLifecycle()

    ChatSearchScreenInternal(
        state = state,
        onQueryChanged = viewModel::onQueryChanged,
        onBackClick = viewModel::onBackClick,
        onResultTapped = viewModel::onResultTapped,
        recentsActions = RecentsActions(
            onTapped = viewModel::onRecentTapped,
            onLongPress = viewModel::onRecentLongPress,
            onDismissMenu = viewModel::onDismissRecentMenu,
            onRemove = viewModel::onRemoveRecent,
            onClearAll = viewModel::onClearRecents,
        ),
    )
}

@Composable
private fun ChatSearchScreenInternal(
    state: ChatSearchUiState,
    onQueryChanged: (String) -> Unit,
    onBackClick: () -> Unit,
    onResultTapped: (ChatSearchRowAction) -> Unit,
    recentsActions: RecentsActions,
) {
    val focusRequester = remember { FocusRequester() }

    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
    }

    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .imePadding()
        ) {
            PolkadotSearchAppBar(
                value = state.query,
                onValueChange = onQueryChanged,
                onClear = { onQueryChanged("") },
                onBack = onBackClick,
                placeholder = stringResource(RCommon.string.common_search),
                focusRequester = focusRequester,
            )

            ChatSearchBody(
                state = state,
                onResultTapped = onResultTapped,
                recentsActions = recentsActions,
            )
        }
    }
}

@Composable
private fun ChatSearchBody(
    state: ChatSearchUiState,
    onResultTapped: (ChatSearchRowAction) -> Unit,
    recentsActions: RecentsActions,
) {
    when (val results = state.results) {
        is SearchState.Initial -> if (state.recents.isEmpty()) {
            ChatSearchPrompt()
        } else {
            RecentsList(recents = state.recents, actions = recentsActions)
        }

        is SearchState.Loading -> Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            NovaCircularProgressIndicator(
                modifier = Modifier.size(32.dp),
                strokeWidth = 3.dp
            )
        }

        is SearchState.Empty -> ChatSearchNoResults(query = state.query)

        is SearchState.Error -> DefaultErrorState(text = stringResource(RCommon.string.chat_search_error_message))

        is SearchState.Loaded -> ChatSearchResultsSections(
            sections = results.results,
            onRowClick = onResultTapped,
        )
    }
}

@Composable
private fun RecentsList(
    recents: ImmutableList<RecentChatUiModel>,
    actions: RecentsActions,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    horizontal = PolkadotTheme.spacings.extraMedium,
                    vertical = PolkadotTheme.spacings.small
                ),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaText(
                text = stringResource(RCommon.string.chat_search_recents_header),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )

            NovaText(
                modifier = Modifier.clickable(onClick = actions.onClearAll),
                text = stringResource(RCommon.string.chat_search_recents_clear),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = LocalAppNavigationBarInsets.current.asPaddingValues()
        ) {
            itemsIndexed(
                items = recents,
                key = { _, recent -> recent.key }
            ) { index, recent ->
                Column {
                    Box {
                        ChatSearchPersonRow(
                            title = recent.title,
                            avatarModel = recent.avatarModel,
                            status = recent.status,
                            onClick = { actions.onTapped(recent.chatId) },
                            onLongClick = { actions.onLongPress(recent.chatId) },
                        )

                        RecentActionMenu(
                            expanded = recent.isMenuOpen,
                            onDismissRequest = actions.onDismissMenu,
                            onRemoveClick = { actions.onRemove(recent.chatId) },
                        )
                    }

                    if (index < recents.lastIndex) {
                        HorizontalDivider(
                            modifier = Modifier.padding(
                                start = ChatSearchPersonAvatarSize + PolkadotTheme.spacings.extraMedium * 2,
                                end = PolkadotTheme.spacings.extraMedium
                            )
                        )
                    }
                }
            }
        }
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ChatSearchScreenPreview() {
    PolkadotTheme {
        ChatSearchScreenInternal(
            state = ChatSearchUiState(
                query = "",
                results = SearchState.Initial,
                recents = persistentListOf(),
            ),
            onQueryChanged = {},
            onBackClick = {},
            onResultTapped = {},
            recentsActions = RecentsActions(
                onTapped = {},
                onLongPress = {},
                onDismissMenu = {},
                onRemove = {},
                onClearAll = {},
            ),
        )
    }
}

@Immutable
private data class RecentsActions(
    val onTapped: (ChatId) -> Unit,
    val onLongPress: (ChatId) -> Unit,
    val onDismissMenu: () -> Unit,
    val onRemove: (ChatId) -> Unit,
    val onClearAll: () -> Unit,
)
