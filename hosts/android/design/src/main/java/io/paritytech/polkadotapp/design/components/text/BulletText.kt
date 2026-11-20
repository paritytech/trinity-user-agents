package io.paritytech.polkadotapp.design.components.text

import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotBulletText(
    text: String,
    style: TextStyle,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.Top
    ) {
        NovaText(
            text = "•",
            style = style,
            color = PolkadotTheme.colors.fg.secondary
        )

        HorizontalSpacer { small }

        NovaText(
            modifier = Modifier.weight(1f),
            text = text,
            style = style,
            color = PolkadotTheme.colors.fg.secondary
        )
    }
}
