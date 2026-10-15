package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ThreadRoomUiState
import io.paritytech.polkadotapp.common.R as RCommon

internal object ProductRoomAdditionsTags {
    const val THREAD_ROOT = "chat_thread_root"
    const val READ_ALOUD_LINE = "chat_read_aloud_line"
    const val READ_ALOUD_STOP = "chat_read_aloud_stop"
}

private const val THREAD_ROOT_LINES = 3
private val QuoteBarWidth = 3.dp

/**
 * The root message of a thread room, pinned as a quote under the header (07-LATER §9.2). A root that the host does
 * not have reads "Earlier message".
 */
@Composable
internal fun ThreadRootBar(
    modifier: Modifier = Modifier,
    thread: ThreadRoomUiState,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .background(PolkadotTheme.colors.bg.surface.container)
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small)
            .testTag(ProductRoomAdditionsTags.THREAD_ROOT),
    ) {
        Box(
            modifier = Modifier
                .width(QuoteBarWidth)
                .fillMaxHeight()
                .background(PolkadotTheme.colors.fg.primary),
        )
        HorizontalSpacer { small }
        Column {
            thread.parentName?.let {
                NovaText(
                    text = it,
                    style = PolkadotTheme.typography.body.smallEmphasized,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            NovaText(
                text = thread.rootText ?: stringResource(RCommon.string.chat_reply_earlier_message),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = THREAD_ROOT_LINES,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** The line in the footer slot while a message of the room speaks, with "Stop" (07-LATER §9.5). */
@Composable
internal fun ReadAloudLine(
    modifier: Modifier = Modifier,
    onStop: () -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = false) { liveRegion = LiveRegionMode.Polite }
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.tiny)
            .testTag(ProductRoomAdditionsTags.READ_ALOUD_LINE),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(RCommon.string.chat_read_aloud_speaking),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary,
        )
        NovaText(
            modifier = Modifier
                .clickable(role = Role.Button, onClick = onStop)
                .padding(PolkadotTheme.spacings.small)
                .testTag(ProductRoomAdditionsTags.READ_ALOUD_STOP),
            text = stringResource(RCommon.string.chat_read_aloud_stop),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}
