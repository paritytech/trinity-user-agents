package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import kotlin.math.roundToInt

internal fun dockletBandPx(barInsetBottom: Int, systemBottom: Int): Int = (barInsetBottom - systemBottom).coerceAtLeast(0)

internal fun visibleDockletBandPx(barInsetBottom: Int, systemBottom: Int, hiddenFraction: Float): Int =
    (dockletBandPx(barInsetBottom, systemBottom) * (1f - hiddenFraction)).roundToInt()
