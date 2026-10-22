package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.badge.IconBadge
import io.paritytech.polkadotapp.design.components.badge.NumberBadge
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.HeartSolid
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState

@Composable
internal fun ChatItemBadges(
    modifier: Modifier = Modifier,
    badge: ChatListUiState.Badge,
    hasReaction: Boolean,
) {
    if (hasReaction || badge is ChatListUiState.Badge.Unread) {
        Row(
            modifier = modifier,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (hasReaction) IconBadge(icon = NovaIcons.HeartSolid)

            when (badge) {
                is ChatListUiState.Badge.Unread -> NumberBadge(number = badge.count)
                ChatListUiState.Badge.None -> Unit
            }
        }
    }
}
