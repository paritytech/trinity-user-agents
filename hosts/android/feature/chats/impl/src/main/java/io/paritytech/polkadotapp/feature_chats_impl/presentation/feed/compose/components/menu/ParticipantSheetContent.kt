package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpRight
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ParticipantSheetEntry
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ParticipantSheetUiState
import io.paritytech.polkadotapp.common.R as RCommon

private val EntryIconSize = 20.dp

/** The sheet of one participant of a product room: its host entry (scenario S31). */
@Composable
fun ParticipantSheetContent(
    state: ParticipantSheetUiState,
    onSelect: (ParticipantSheetEntry) -> Unit,
) {
    Column(modifier = Modifier.padding(vertical = PolkadotTheme.spacings.mediumIncreased)) {
        ParticipantRow(participant = state.participant, onClick = null)

        state.entries.forEach { entry ->
            when (entry) {
                is ParticipantSheetEntry.CopyAddress -> SheetEntry(
                    text = stringResource(RCommon.string.chat_participant_copy_address),
                    color = PolkadotTheme.colors.fg.primary,
                    onClick = { onSelect(entry) },
                ) { NovaIcon(modifier = Modifier.size(EntryIconSize), imageVector = NovaIcons.ContentCopy, tint = PolkadotTheme.colors.fg.primary) }

                is ParticipantSheetEntry.OpenProduct -> SheetEntry(
                    text = stringResource(RCommon.string.chat_participant_open_product),
                    color = PolkadotTheme.colors.fg.primary,
                    onClick = { onSelect(entry) },
                ) { NovaIcon(modifier = Modifier.size(EntryIconSize), imageVector = NovaIcons.ArrowUpRight, tint = PolkadotTheme.colors.fg.primary) }
            }
        }
    }
}

@Composable
private fun SheetEntry(
    text: String,
    color: Color,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = PolkadotTheme.spacings.large, vertical = PolkadotTheme.spacings.medium),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(modifier = Modifier.size(EntryIconSize)) { icon() }

        HorizontalSpacer { smallIncreased }

        NovaText(text = text, style = PolkadotTheme.typography.body.large, color = color)
    }
}
