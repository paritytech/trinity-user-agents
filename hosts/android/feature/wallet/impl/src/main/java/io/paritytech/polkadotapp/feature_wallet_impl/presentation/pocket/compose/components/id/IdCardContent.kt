package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.id

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.QrCode
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.CardSizes
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors

@Composable
internal fun IdCardContent(
    username: String,
    avatarPainter: Painter,
    primaryTextColor: Color,
    onQrClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(CardSizes.HEIGHT)
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top
    ) {
        Row(
            modifier = Modifier.weight(1f, false),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Image(
                modifier = Modifier.size(50.dp),
                painter = avatarPainter,
                contentDescription = "avatar"
            )

            NovaText(
                modifier = Modifier.testTag(PocketTestTags.USERNAME_DISPLAY),
                text = username,
                style = PolkadotTheme.typography.title.large,
                color = primaryTextColor,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
        }

        PolkadotSurface(
            color = PocketCardColors.Transparent,
            contentColor = PocketCardColors.Secondary,
            onClick = onQrClick
        ) {
            NovaIcon(
                modifier = Modifier
                    .padding(PolkadotTheme.spacings.smallIncreased)
                    .size(24.dp),
                imageVector = NovaIcons.QrCode
            )
        }
    }
}
