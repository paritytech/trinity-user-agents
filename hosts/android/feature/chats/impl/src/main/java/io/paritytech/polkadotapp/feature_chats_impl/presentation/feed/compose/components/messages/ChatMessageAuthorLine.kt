package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantUiModel

private val AuthorAvatarSize = 20.dp

/** The participant of a product room who wrote the messages below (02-DESIGN §3.3.7). */
@Composable
internal fun ChatMessageAuthorLine(
    modifier: Modifier = Modifier,
    author: ChatParticipantUiModel,
    onClick: () -> Unit,
) {
    Row(
        modifier = modifier
            .clickable(onClick = onClick)
            .padding(vertical = PolkadotTheme.spacings.tiny),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PolkadotAvatar(model = author.avatar, modifier = Modifier.size(AuthorAvatarSize))

        HorizontalSpacer { small }

        NovaText(
            text = author.name,
            style = PolkadotTheme.typography.body.smallEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
