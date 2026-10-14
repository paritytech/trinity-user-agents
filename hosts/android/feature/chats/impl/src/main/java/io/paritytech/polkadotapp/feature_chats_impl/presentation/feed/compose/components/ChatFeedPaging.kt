package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatPageEdgeUiState
import kotlinx.coroutines.flow.distinctUntilChanged
import io.paritytech.polkadotapp.common.R as RCommon

/** The feed asks the next page when this number of items or fewer lies between the screen and the end. */
private const val PAGE_LOAD_DISTANCE = 10
private const val OLDER_PAGE_KEY = "chat_older_page"
private const val NEWER_PAGE_KEY = "chat_newer_page"

private val PageProgressSize = 24.dp

internal val ChatPageEdgeUiState.showsRow: Boolean
    get() = this == ChatPageEdgeUiState.LOADING || this == ChatPageEdgeUiState.FAILED

/** The row below the newest message while newer messages load. With `reverseLayout`, this item goes first. */
internal fun LazyListScope.newerPageRow(state: ChatPageEdgeUiState, onRetry: () -> Unit) {
    if (state.showsRow) item(key = NEWER_PAGE_KEY) { ChatPageEdgeRow(state = state, onRetry = onRetry) }
}

/** The row above the oldest message while older messages load. With `reverseLayout`, this item goes last. */
internal fun LazyListScope.olderPageRow(state: ChatPageEdgeUiState, onRetry: () -> Unit) {
    if (state.showsRow) item(key = OLDER_PAGE_KEY) { ChatPageEdgeRow(state = state, onRetry = onRetry) }
}

/**
 * Asks the next page when the user comes near an end of the loaded messages. The list keeps its position when
 * a page arrives, because each message has a stable key.
 */
@Composable
internal fun ChatPagingTriggers(
    lazyListState: LazyListState,
    olderPage: ChatPageEdgeUiState,
    newerPage: ChatPageEdgeUiState,
    onLoadOlder: () -> Unit,
    onLoadNewer: () -> Unit,
) {
    val currentOlderPage by rememberUpdatedState(olderPage)
    val currentNewerPage by rememberUpdatedState(newerPage)
    val currentOnLoadOlder by rememberUpdatedState(onLoadOlder)
    val currentOnLoadNewer by rememberUpdatedState(onLoadNewer)

    LaunchedEffect(lazyListState) {
        snapshotFlow {
            val layoutInfo = lazyListState.layoutInfo
            val lastVisible = layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: -1
            val nearOldest = layoutInfo.totalItemsCount > 0 && lastVisible >= layoutInfo.totalItemsCount - PAGE_LOAD_DISTANCE
            nearOldest && currentOlderPage == ChatPageEdgeUiState.MORE
        }
            .distinctUntilChanged()
            .collect { shouldLoad -> if (shouldLoad) currentOnLoadOlder() }
    }

    LaunchedEffect(lazyListState) {
        snapshotFlow {
            lazyListState.firstVisibleItemIndex <= PAGE_LOAD_DISTANCE && currentNewerPage == ChatPageEdgeUiState.MORE
        }
            .distinctUntilChanged()
            .collect { shouldLoad -> if (shouldLoad) currentOnLoadNewer() }
    }
}

@Composable
private fun ChatPageEdgeRow(state: ChatPageEdgeUiState, onRetry: () -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.medium),
        contentAlignment = Alignment.Center,
    ) {
        if (state == ChatPageEdgeUiState.FAILED) {
            NovaText(
                modifier = Modifier.clickable(onClick = onRetry),
                text = stringResource(RCommon.string.chat_feed_page_failed),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.Center,
            )
        } else {
            NovaCircularProgressIndicator(modifier = Modifier.size(PageProgressSize), strokeWidth = 2.dp)
        }
    }
}
