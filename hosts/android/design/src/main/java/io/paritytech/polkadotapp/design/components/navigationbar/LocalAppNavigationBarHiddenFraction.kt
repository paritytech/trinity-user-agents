package io.paritytech.polkadotapp.design.components.navigationbar

import androidx.compose.runtime.compositionLocalOf

val LocalAppNavigationBarHiddenFraction = compositionLocalOf<() -> Float> { { 0f } }
