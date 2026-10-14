package io.paritytech.polkadotapp.design.components.button.common

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Shape
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

object PolkadotButtonShape {
    val rounded: Shape
        @Composable
        @ReadOnlyComposable
        get() = PolkadotTheme.shapes.medium

    val pill: Shape
        @Composable
        @ReadOnlyComposable
        get() = PolkadotTheme.shapes.full
}
