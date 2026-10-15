package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ParticipantListUiState
import io.paritytech.polkadotapp.common.R as RCommon

private val ParticipantAvatarSize = 40.dp
private val ParticipantListMaxHeight = 480.dp

/** The participants of a product room, page after page (02-DESIGN §3.3.7, scenario S30). */
@Composable
fun ParticipantsListContent(
    state: ParticipantListUiState,
    onLoadMore: () -> Unit,
    onSelect: (participantId: String) -> Unit,
) {
    Column(modifier = Modifier.padding(vertical = PolkadotTheme.spacings.mediumIncreased)) {
        NovaText(
            modifier = Modifier
                .padding(horizontal = PolkadotTheme.spacings.large)
                .semantics { heading() },
            text = stringResource(RCommon.string.chat_participants_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
        )

        LazyColumn(modifier = Modifier.heightIn(max = ParticipantListMaxHeight)) {
            items(items = state.participants, key = { it.participantId }) { participant ->
                ParticipantRow(participant = participant, onClick = { onSelect(participant.participantId) })
            }

            if (state.hasMore || state.failed) {
                item(key = "participants_paging") {
                    ParticipantsPagingRow(state = state, onLoadMore = onLoadMore)
                }
            }
        }
    }
}

@Composable
internal fun ParticipantRow(
    participant: ChatParticipantUiModel,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (onClick != null) {
                    Modifier.clickable(onClickLabel = stringResource(RCommon.string.chat_participant_show), onClick = onClick)
                } else {
                    Modifier.semantics(mergeDescendants = true) { }
                },
            )
            .padding(horizontal = PolkadotTheme.spacings.large, vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PolkadotAvatar(model = participant.avatar, modifier = Modifier.size(ParticipantAvatarSize))

        HorizontalSpacer { smallIncreased }

        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = participant.name,
                style = PolkadotTheme.typography.body.largeEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )

            if (participant.detail.isNotEmpty()) {
                NovaText(
                    text = participant.detail,
                    style = PolkadotTheme.typography.body.small,
                    color = PolkadotTheme.colors.fg.tertiary,
                )
            }
        }
    }
}

@Composable
private fun ParticipantsPagingRow(
    state: ParticipantListUiState,
    onLoadMore: () -> Unit,
) {
    if (state.failed) {
        NovaText(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(role = Role.Button, onClick = onLoadMore)
                .padding(PolkadotTheme.spacings.large),
            text = stringResource(RCommon.string.chat_participants_failed),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
    } else {
        LaunchedEffect(state.participants.size) { onLoadMore() }

        val loading = stringResource(RCommon.string.chat_product_menu_loading)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .semantics { contentDescription = loading }
                .padding(PolkadotTheme.spacings.mediumIncreased),
            contentAlignment = Alignment.Center,
        ) {
            NovaCircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 2.dp)
        }
    }
}
