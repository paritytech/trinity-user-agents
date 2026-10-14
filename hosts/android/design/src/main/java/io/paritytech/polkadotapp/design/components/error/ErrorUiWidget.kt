package io.paritytech.polkadotapp.design.components.error

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AlertFilled
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun DefaultErrorState(
    text: String,
    modifier: Modifier = Modifier
        .fillMaxSize()
        .systemBarsPadding()
) {
    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            NovaIcon(
                modifier = Modifier.size(64.dp),
                contentDescription = null,
                imageVector = NovaIcons.AlertFilled,
                tint = PolkadotTheme.colors.fg.secondary
            )

            VerticalSpacer { mediumIncreased }

            NovaText(
                modifier = Modifier.padding(horizontal = 36.dp),
                text = text,
                style = PolkadotTheme.typography.body.large,
                textAlign = TextAlign.Center,
                color = PolkadotTheme.colors.fg.secondary
            )
        }
    }
}

@Preview
@Composable
fun DefaultErrorStatePreview() {
    PolkadotTheme {
        DefaultErrorState("Default error")
    }
}
