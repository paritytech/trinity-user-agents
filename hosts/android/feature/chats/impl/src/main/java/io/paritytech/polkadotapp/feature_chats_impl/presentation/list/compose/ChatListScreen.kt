package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.ChatListViewModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatListContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatListHeader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatListLoading
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.IdleSearchEntry
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.SearchRevealContainer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.rememberSearchRevealState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.collections.immutable.persistentListOf

@Composable
fun ChatListScreen() {
    val viewModel = hiltViewModel<ChatListViewModel>()
    val state by viewModel.state.collectAsStateWithLifecycle()

    ChatListScreenInternal(
        loadingState = state,
        onAddContactClick = viewModel::onAddContactClick,
        onChatClick = viewModel::onChatClick,
        onNewRequestsClick = viewModel::onNewRequestsClick,
        onSearchBarClick = viewModel::onSearchBarClick,
    )
}

@Composable
private fun ChatListScreenInternal(
    loadingState: LoadingState<ChatListUiState>,
    onAddContactClick: () -> Unit,
    onChatClick: (ChatListUiState.ChatItem) -> Unit,
    onNewRequestsClick: () -> Unit,
    onSearchBarClick: () -> Unit,
) {
    val revealState = rememberSearchRevealState()

    PolkadotSurface {
        Column(
            modifier = Modifier.fillMaxSize()
        ) {
            ChatListHeader(
                onAddContactClick = onAddContactClick,
                isLoading = loadingState is LoadingState.Loading
            )

            when (loadingState) {
                is LoadingState.Loaded -> SearchRevealContainer(
                    state = revealState,
                    header = { IdleSearchEntry(onClick = onSearchBarClick) },
                    content = {
                        ChatListContent(
                            state = loadingState.data,
                            onChatClick = onChatClick,
                            onNewRequestsClick = onNewRequestsClick
                        )
                    },
                )

                else -> ChatListLoading()
            }
        }
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ChatListScreenPreview() {
    PolkadotTheme {
        ChatListScreenInternal(
            loadingState = LoadingState.Loaded(
                ChatListUiState(
                    chats = persistentListOf(),
                    pendingRequestsCount = 1
                )
            ),
            onAddContactClick = {},
            onChatClick = {},
            onNewRequestsClick = {},
            onSearchBarClick = {},
        )
    }
}
