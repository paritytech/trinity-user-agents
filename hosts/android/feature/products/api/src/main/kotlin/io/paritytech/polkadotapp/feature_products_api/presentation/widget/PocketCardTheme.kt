package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme

/** Draws a Pocket card in Berlin Night whatever theme the user picked, as the Pocket RFC promises. */
@Composable
fun PocketCardTheme(content: @Composable () -> Unit) {
    PolkadotTheme(theme = PolkadotAppTheme.BerlinNight, content = content)
}
