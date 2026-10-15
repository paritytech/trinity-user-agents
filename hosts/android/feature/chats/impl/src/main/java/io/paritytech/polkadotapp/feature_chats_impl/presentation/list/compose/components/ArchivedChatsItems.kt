@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDragHandler
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.icon.vectors.Downloading
import io.paritytech.polkadotapp.design.components.icon.vectors.SpeakerXMark
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.common.R as RCommon

/** The test tag of the "Archived" row at the top of the chat list. */
internal const val ARCHIVED_CHATS_ROW_TAG = "chat_list_archived_row"

/** The test tag of the header of the archived section. */
internal const val ARCHIVED_CHATS_HEADER_TAG = "chat_list_archived_header"

/** The row at the top of the chat list that opens the archived rooms (07-LATER §6.2). */
@Composable
internal fun ArchivedChatsRow(
    modifier: Modifier = Modifier,
    count: Int,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = modifier.testTag(ARCHIVED_CHATS_ROW_TAG),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = PolkadotTheme.spacings.extraMedium),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            HorizontalSpacer { 20.dp }

            NovaIcon(imageVector = NovaIcons.Downloading, tint = PolkadotTheme.colors.fg.primary)

            HorizontalSpacer { mediumIncreased }

            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.chat_list_archived),
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary,
            )

            NovaText(
                text = count.toString(),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.tertiary,
            )

            HorizontalSpacer { small }

            NovaIcon(
                modifier = Modifier.size(16.dp),
                imageVector = NovaIcons.ArrowRight,
                tint = PolkadotTheme.colors.fg.secondary,
            )

            HorizontalSpacer { mediumIncreased }
        }
    }
}

/** The header of the archived section. Its arrow, and the system Back, return to the main section. */
@Composable
internal fun ArchivedChatsHeader(
    modifier: Modifier = Modifier,
    onBack: () -> Unit,
) {
    Row(
        modifier = modifier
            .testTag(ARCHIVED_CHATS_HEADER_TAG)
            .fillMaxWidth()
            .clickable(onClick = onBack)
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.medium),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaIcon(imageVector = NovaIcons.ArrowLeft, tint = PolkadotTheme.colors.fg.primary)

        HorizontalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.chat_list_archived),
            style = PolkadotTheme.typography.title.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

/** The durations of a mute (07-LATER §6.2): 1 hour, 8 hours, 1 week, or always. */
@Composable
internal fun ChatMuteDurationSheet(
    isVisible: Boolean,
    onSelect: (ChatMuteDuration) -> Unit,
    onDismiss: () -> Unit,
) {
    NovaModalBottomSheet(isVisible = isVisible, onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = PolkadotTheme.spacings.small, bottom = PolkadotTheme.spacings.mediumIncreased),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            NovaBottomSheetDragHandler()

            NovaText(
                modifier = Modifier.padding(vertical = PolkadotTheme.spacings.medium),
                text = stringResource(RCommon.string.chat_room_mute_title),
                style = PolkadotTheme.typography.headline.small,
                color = PolkadotTheme.colors.fg.primary,
            )

            ChatMuteDuration.entries.forEach { duration ->
                NovaMenuOption(
                    text = stringResource(duration.label()),
                    icon = if (duration == ChatMuteDuration.ALWAYS) NovaIcons.SpeakerXMark else NovaIcons.Clock,
                    color = PolkadotTheme.colors.fg.primary,
                    onClick = { onSelect(duration) },
                )
            }
        }
    }
}

private fun ChatMuteDuration.label(): Int = when (this) {
    ChatMuteDuration.ONE_HOUR -> RCommon.string.chat_room_mute_one_hour
    ChatMuteDuration.EIGHT_HOURS -> RCommon.string.chat_room_mute_eight_hours
    ChatMuteDuration.ONE_WEEK -> RCommon.string.chat_room_mute_one_week
    ChatMuteDuration.ALWAYS -> RCommon.string.chat_room_mute_always
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ArchivedChatsRowPreview() {
    PolkadotTheme {
        Column {
            ArchivedChatsRow(count = 3, onClick = {})
            ArchivedChatsHeader(onBack = {})
        }
    }
}
