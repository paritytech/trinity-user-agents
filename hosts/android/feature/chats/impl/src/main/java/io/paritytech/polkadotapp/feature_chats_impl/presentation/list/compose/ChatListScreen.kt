package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.docklet.DockletGlassTheme
import io.paritytech.polkadotapp.design.components.docklet.LocalDockletPalette
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletGlassButtonStyle
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Add
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
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
    val density = LocalDensity.current
    var newChatButtonHeight by remember { mutableStateOf(0.dp) }
    val newChatButtonOffset = PolkadotTheme.spacings.mediumIncreased

    PolkadotSurface {
        Box(modifier = Modifier.fillMaxSize()) {
            Column(
                modifier = Modifier.fillMaxSize()
            ) {
                ChatListHeader(isLoading = loadingState is LoadingState.Loading)

                when (loadingState) {
                    is LoadingState.Loaded -> SearchRevealContainer(
                        state = revealState,
                        header = { IdleSearchEntry(onClick = onSearchBarClick) },
                        content = {
                            ChatListContent(
                                state = loadingState.data,
                                onChatClick = onChatClick,
                                onNewRequestsClick = onNewRequestsClick,
                                bottomClearance = newChatButtonHeight + newChatButtonOffset,
                            )
                        },
                    )

                    else -> ChatListLoading()
                }
            }

            NewChatButton(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .windowInsetsPadding(LocalAppNavigationBarInsets.current)
                    .padding(newChatButtonOffset)
                    .onSizeChanged { newChatButtonHeight = with(density) { it.height.toDp() } },
                onClick = onAddContactClick,
            )
        }
    }
}

@Composable
private fun NewChatButton(
    modifier: Modifier,
    onClick: () -> Unit,
) {
    DockletGlassTheme {
        val borderWidth = PolkadotTheme.borders.default
        val borderColor = LocalDockletPalette.current.glassStroke
        val border = remember(borderWidth, borderColor) { BorderStroke(borderWidth, borderColor) }
        PolkadotIconButton(
            icon = NovaIcons.Add,
            onClick = onClick,
            modifier = modifier,
            style = rememberDockletGlassButtonStyle(),
            size = PolkadotIconButtonSize.medium(),
            shape = PolkadotButtonShape.pill,
            border = border,
        )
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
