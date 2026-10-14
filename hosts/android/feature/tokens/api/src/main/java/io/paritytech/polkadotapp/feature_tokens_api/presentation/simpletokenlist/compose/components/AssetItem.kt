package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig

@Composable
fun AssetItem(
    uiConfig: KnownTokenUiConfig,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                vertical = PolkadotTheme.spacings.tiny,
                horizontal = PolkadotTheme.spacings.mediumIncreased
            ),
        color = Color(0x1FFFFFFF),
        shape = PolkadotTheme.shapes.large,
        onClick = onClick
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(
                modifier = Modifier.size(48.dp),
                imageVector = uiConfig.icon,
                contentDescription = "token_icon"
            )

            HorizontalSpacer { mediumIncreased }

            Column(
                modifier = Modifier.weight(1f)
            ) {
                NovaText(
                    text = uiConfig.symbol,
                    style = PolkadotTheme.typography.title.large,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                )
                val name = uiConfig.name
                if (name != null) {
                    VerticalSpacer { tiny }

                    NovaText(
                        text = name,
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.tertiary,
                        maxLines = 1,
                    )
                }
            }

            NovaIcon(
                modifier = Modifier.size(24.dp),
                imageVector = NovaIcons.ArrowRight,
                tint = PolkadotTheme.colors.fg.tertiary,
                contentDescription = "arrow"
            )
        }
    }
}
