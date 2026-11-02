package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.SpeakerXMark
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchRowStatus
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatItemBadges

internal val ChatSearchPersonAvatarSize = 40.dp
private val MuteIconSize = 20.dp

@Composable
internal fun ChatSearchPersonRow(
    title: String,
    avatarModel: AvatarUiModel,
    status: ChatSearchRowStatus,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(PolkadotTheme.spacings.extraMedium),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PolkadotAvatar(
            modifier = Modifier.size(ChatSearchPersonAvatarSize),
            model = avatarModel,
        )

        Row(
            modifier = Modifier.weight(1f),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaText(
                modifier = Modifier.weight(1f, fill = false),
                text = title,
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )

            if (status.isMuted) {
                NovaIcon(
                    modifier = Modifier.size(MuteIconSize),
                    imageVector = NovaIcons.SpeakerXMark,
                    tint = PolkadotTheme.colors.fg.tertiary,
                )
            }
        }

        ChatItemBadges(
            badge = status.badge,
            hasReaction = status.hasReaction,
        )
    }
}
