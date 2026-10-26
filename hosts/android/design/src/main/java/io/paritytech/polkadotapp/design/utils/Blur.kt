package io.paritytech.polkadotapp.design.utils

import android.os.Build
import androidx.compose.ui.graphics.BlurEffect
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.unit.Dp
import kotlin.math.max

// hwui turns a blur radius into σ as 0.57735 · radius + 0.5, in px (frameworks/base libs/hwui/utils/Blur.cpp).
private const val BlurSigmaScale = 0.57735f
private const val BlurSigmaOffset = 0.5f

internal fun blurSupported(sdkInt: Int): Boolean = sdkInt >= Build.VERSION_CODES.S

internal fun blurRadiusPx(sigmaDp: Float, density: Float): Float =
    max(0f, (sigmaDp * density - BlurSigmaOffset) / BlurSigmaScale)

fun blurEffect(sigma: Dp, density: Float): BlurEffect? {
    if (!blurSupported(Build.VERSION.SDK_INT)) return null

    val radius = blurRadiusPx(sigma.value, density)
    return if (radius > 0f) BlurEffect(radius, radius, TileMode.Decal) else null
}
