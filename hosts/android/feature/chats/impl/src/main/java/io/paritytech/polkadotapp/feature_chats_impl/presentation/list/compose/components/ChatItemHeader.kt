package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.SpeakerXMark
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val MuteIconSize = 20.dp

@Composable
internal fun ChatItemHeader(
    title: String,
    timestamp: String,
    isMuted: Boolean,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.Bottom,
    ) {
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

            if (isMuted) {
                NovaIcon(
                    modifier = Modifier.size(MuteIconSize),
                    imageVector = NovaIcons.SpeakerXMark,
                    tint = PolkadotTheme.colors.fg.tertiary,
                )
            }
        }

        NovaText(
            text = timestamp,
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.tertiary,
            maxLines = 1,
        )
    }
}
