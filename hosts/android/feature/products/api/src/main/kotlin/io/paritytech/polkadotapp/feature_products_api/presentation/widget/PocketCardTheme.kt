package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme

/** Draws a Pocket card face in the default theme, so a card looks the same whatever theme the user picked. */
@Composable
fun PocketCardTheme(content: @Composable () -> Unit) {
    PolkadotTheme(theme = PolkadotAppTheme.DEFAULT, content = content)
}
