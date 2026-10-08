package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import io.paritytech.polkadotapp.designsystem.colors.LocalPolkadotColors
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme

/** Draws a Pocket card face in the default theme, so a card looks the same whatever theme the user picked. */
@Composable
fun PocketCardTheme(content: @Composable () -> Unit) {
    val palette = remember { PolkadotAppTheme.DEFAULT.colors() }
    CompositionLocalProvider(LocalPolkadotColors provides palette, content = content)
}
