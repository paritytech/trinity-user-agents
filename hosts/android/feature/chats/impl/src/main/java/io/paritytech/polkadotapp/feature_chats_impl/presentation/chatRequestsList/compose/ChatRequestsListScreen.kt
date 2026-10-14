@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.randomBytes
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.ChatRequestsListContract
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose.components.ChatRequestListItem
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose.components.DeclineConfirmationContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestsListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatListLoading
import kotlinx.collections.immutable.persistentListOf
import kotlin.random.Random
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ChatRequestsListScreen(contract: ChatRequestsListContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    val declineConfirmation by contract.declineConfirmationState.collectAsStateWithLifecycle()

    ChatRequestsListScreenInternal(
        loadingState = state,
        onRequestClick = contract::onRequestClick,
        onDeclineClick = contract::onDeclineClick,
        onBackClick = contract::onBackClick
    )

    NovaModalBottomSheet(
        isVisible = declineConfirmation.isVisible,
        onDismissRequest = contract::onCancelDeclineClick
    ) {
        DeclineConfirmationContent(
            onCancel = contract::onCancelDeclineClick,
            onConfirm = contract::onConfirmDeclineClick,
        )
    }
}

@Composable
private fun ChatRequestsListScreenInternal(
    loadingState: LoadingState<ChatRequestsListUiState>,
    onRequestClick: (ChatRequestsListUiState.ChatRequestItem) -> Unit,
    onDeclineClick: (ChatRequestsListUiState.ChatRequestItem) -> Unit,
    onBackClick: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            ChatRequestsTopBar(
                onBackClick = onBackClick
            )

            when (loadingState) {
                is LoadingState.Loaded -> ChatRequestsListContent(
                    state = loadingState.data,
                    onRequestClick = onRequestClick,
                    onDeclineClick = onDeclineClick
                )

                else -> ChatListLoading()
            }
        }
    }
}

@Composable
private fun ChatRequestsTopBar(
    onBackClick: () -> Unit
) {
    PolkadotTopBar(
        title = stringResource(RCommon.string.chat_request_message_requests),
        navigationAction = rememberTopBarAction(onBackClick),
        titleAlignment = TopBarTitleAlignment.Center,
    )
}

@Composable
private fun ChatRequestsListContent(
    state: ChatRequestsListUiState,
    onRequestClick: (ChatRequestsListUiState.ChatRequestItem) -> Unit,
    onDeclineClick: (ChatRequestsListUiState.ChatRequestItem) -> Unit
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(vertical = PolkadotTheme.spacings.extraMedium)
    ) {
        item {
            SubtitleBanner()
        }

        itemsIndexed(
            items = state.requests,
            key = { _, request -> request.uniqueKey }
        ) { index, request ->
            ChatRequestListItem(
                modifier = Modifier.animateItem(),
                request = request,
                onClick = { onRequestClick(request) },
                onDeclineClick = { onDeclineClick(request) }
            )

            if (index < state.requests.lastIndex) {
                HorizontalDivider(
                    modifier = Modifier.padding(
                        start = 88.dp,
                        end = PolkadotTheme.spacings.mediumIncreased
                    ),
                    color = Color(0x1FFFFFFF)
                )
            }
        }
    }
}

@Composable
private fun SubtitleBanner() {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.small
            ),
        shape = PolkadotTheme.shapes.medium,
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        NovaText(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
            text = stringResource(RCommon.string.chat_request_message_requests_subtitle),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ChatRequestsListScreenLoadedPreview() {
    ChatRequestsListScreenPreview {
        ChatRequestsListScreenInternal(
            loadingState = LoadingState.Loaded(
                ChatRequestsListUiState(
                    requests = persistentListOf(
                        ChatRequestsListUiState.ChatRequestItem(
                            accountId = Random.randomBytes(32).intoAccountId(),
                            display = ChatDisplayUiModel(
                                username = "alice.polkadot",
                                avatarModel = AvatarUiModel.Mock.fromName("alice.polkadot")
                            ),
                            timestamp = System.currentTimeMillis()
                        ),
                        ChatRequestsListUiState.ChatRequestItem(
                            accountId = Random.randomBytes(32).intoAccountId(),
                            display = ChatDisplayUiModel(
                                username = "bob.polkadot",
                                avatarModel = AvatarUiModel.Mock.fromName("bob.polkadot")
                            ),
                            timestamp = System.currentTimeMillis()
                        )
                    )
                )
            ),
            onRequestClick = {},
            onDeclineClick = {},
            onBackClick = {}
        )
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ChatRequestsListScreenEmptyPreview() {
    ChatRequestsListScreenPreview {
        ChatRequestsListScreenInternal(
            loadingState = LoadingState.Loaded(
                ChatRequestsListUiState(requests = persistentListOf())
            ),
            onRequestClick = {},
            onDeclineClick = {},
            onBackClick = {}
        )
    }
}

@Composable
private fun ChatRequestsListScreenPreview(
    content: @Composable () -> Unit
) {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            content()
        }
    }
}
