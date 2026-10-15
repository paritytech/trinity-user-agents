package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.badge.IconBadge
import io.paritytech.polkadotapp.design.components.badge.NumberBadge
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.HeartSolid
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState

private val BadgeImageSize = 20.dp

@Composable
internal fun ChatItemBadges(
    modifier: Modifier = Modifier,
    badge: ChatListUiState.Badge,
    hasReaction: Boolean,
    isMuted: Boolean = false,
) {
    if (hasReaction || badge !is ChatListUiState.Badge.None) {
        Row(
            modifier = modifier,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (hasReaction) IconBadge(icon = NovaIcons.HeartSolid)

            when (badge) {
                // A muted room shows its count in grey (07-LATER §6.2).
                is ChatListUiState.Badge.Unread -> if (isMuted) MutedNumberBadge(number = badge.count) else NumberBadge(number = badge.count)
                is ChatListUiState.Badge.Image -> NovaAsyncImage(
                    modifier = Modifier
                        .size(BadgeImageSize)
                        .clip(PolkadotTheme.shapes.full),
                    model = badge.image.model,
                    contentScale = ContentScale.Crop,
                )
                ChatListUiState.Badge.None -> Unit
            }
        }
    }
}

/** The tag of the grey count of a muted room; the launcher checks the colour of the badge with it. */
internal const val MUTED_BADGE_TAG = "chat_list_muted_badge"

@Composable
private fun MutedNumberBadge(number: Int) {
    PolkadotSurface(
        modifier = Modifier.testTag(MUTED_BADGE_TAG),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.fg.tertiary,
    ) {
        Box(
            modifier = Modifier
                .defaultMinSize(minWidth = 20.dp, minHeight = 20.dp)
                .padding(vertical = PolkadotTheme.spacings.extraTiny, horizontal = PolkadotTheme.spacings.tiny),
            contentAlignment = Alignment.Center,
        ) {
            NovaText(
                text = number.toString(),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.primaryInverted,
                maxLines = 1,
            )
        }
    }
}
