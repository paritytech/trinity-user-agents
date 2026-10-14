package io.paritytech.polkadotapp.design.components.empty

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatFilled
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun EmptyScreenState(
    modifier: Modifier = Modifier,
    illustration: Painter,
    title: String,
    message: String
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Image(
            modifier = Modifier.size(IllustrationSize),
            painter = illustration,
            contentDescription = null,
            colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.secondary)
        )
        VerticalSpacer { large }
        NovaText(
            text = title,
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )
        VerticalSpacer { small }
        NovaText(
            text = message,
            style = PolkadotTheme.typography.paragraph.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )
    }
}

private val IllustrationSize = 80.dp

@Preview(backgroundColor = 0xFF191919, showBackground = true)
@Composable
private fun EmptyScreenStatePreview() {
    PolkadotTheme {
        EmptyScreenState(
            modifier = Modifier.fillMaxSize(),
            illustration = rememberVectorPainter(NovaIcons.ChatFilled),
            title = "Add your first contact",
            message = "Tap + below to find someone by username"
        )
    }
}
