package io.paritytech.polkadotapp.feature_backup_impl.recover.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun OptionButton(
    modifier: Modifier = Modifier,
    title: String,
    description: String,
    onAction: () -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        color = Color(0x0FFFFFFF),
        shape = RoundedCornerShape(22.dp),
        onClick = onAction
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    top = PolkadotTheme.spacings.large,
                    bottom = PolkadotTheme.spacings.large,
                    start = PolkadotTheme.spacings.large,
                    end = PolkadotTheme.spacings.mediumIncreased,
                ),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
            ) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.title.medium,
                    color = PolkadotTheme.colors.fg.primary
                )
                NovaText(
                    text = description,
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary
                )
            }

            PolkadotSurface(
                color = Color(0x0FFFFFFF),
                shape = PolkadotTheme.shapes.full
            ) {
                NovaIcon(
                    modifier = Modifier
                        .padding(PolkadotTheme.spacings.extraMedium)
                        .size(20.dp),
                    imageVector = NovaIcons.ArrowRight,
                    tint = PolkadotTheme.colors.fg.tertiary
                )
            }
        }
    }
}

@Preview
@Composable
private fun OptionButtonPreview() {
    PolkadotTheme {
        OptionButton(
            title = "Button title",
            description = "Button description",
            onAction = {}
        )
    }
}
