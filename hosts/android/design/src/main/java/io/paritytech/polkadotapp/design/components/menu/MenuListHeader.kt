package io.paritytech.polkadotapp.design.components.menu

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotMenuListHeader(
    modifier: Modifier = Modifier,
    text: String
) {
    NovaText(
        modifier = modifier,
        text = text.uppercase(),
        style = PolkadotTheme.typography.caption.medium
    )
}
