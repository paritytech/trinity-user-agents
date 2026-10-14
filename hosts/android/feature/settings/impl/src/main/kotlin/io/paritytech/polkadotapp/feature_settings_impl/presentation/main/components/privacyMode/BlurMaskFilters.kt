package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import android.graphics.BlurMaskFilter
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp

@Composable
internal fun rememberBlurMaskFilter(radius: Dp): BlurMaskFilter {
    val density = LocalDensity.current

    return remember(density) {
        with(density) { BlurMaskFilter(radius.toPx(), BlurMaskFilter.Blur.NORMAL) }
    }
}
