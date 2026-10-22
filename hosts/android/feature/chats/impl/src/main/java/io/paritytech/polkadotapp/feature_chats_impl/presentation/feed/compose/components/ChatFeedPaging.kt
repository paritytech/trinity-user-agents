package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatPageEdgeUiState
import kotlinx.coroutines.flow.distinctUntilChanged

/** The feed asks the next page when this number of items or fewer lies between the screen and the end. */
private const val PAGE_LOAD_DISTANCE = 10

/**
 * Asks the next page when the user comes near an end of the loaded messages. The list keeps its position when
 * a page arrives, because each message has a stable key. As the native list, it shows no loading row and no failure:
 * the source asks a failed page again after a delay.
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
