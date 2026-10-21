package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

internal object ProductRoomAdditionsTags {
    const val READ_ALOUD_LINE = "chat_read_aloud_line"
    const val READ_ALOUD_STOP = "chat_read_aloud_stop"
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
