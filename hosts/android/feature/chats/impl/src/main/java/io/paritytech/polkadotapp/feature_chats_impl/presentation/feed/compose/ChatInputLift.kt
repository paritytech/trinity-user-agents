package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Density
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.visibleDockletBandPx

internal fun Modifier.shrinkAboveDocklet(
    barInsets: WindowInsets,
    navigationBars: WindowInsets,
    ime: WindowInsets,
    hiddenFraction: () -> Float,
): Modifier = layout { measurable, constraints ->
    val lift = visibleLiftPx(barInsets, navigationBars, ime, hiddenFraction())
    val height = (constraints.maxHeight - lift).coerceAtLeast(0)
    val placeable = measurable.measure(constraints.copy(minHeight = height, maxHeight = height))
    layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(0, 0) }
}

internal fun Modifier.liftAboveDocklet(
    barInsets: WindowInsets,
    navigationBars: WindowInsets,
    ime: WindowInsets,
    hiddenFraction: () -> Float,
): Modifier = layout { measurable, constraints ->
    val lift = visibleLiftPx(barInsets, navigationBars, ime, hiddenFraction())
    val placeable = measurable.measure(constraints.copy(minHeight = 0))
    val height = (placeable.height + lift).coerceIn(constraints.minHeight, constraints.maxHeight)
    layout(placeable.width, height) { placeable.place(0, 0) }
}

private fun Density.visibleLiftPx(
    barInsets: WindowInsets,
    navigationBars: WindowInsets,
    ime: WindowInsets,
    hiddenFraction: Float,
): Int {
    val systemBottom = maxOf(navigationBars.getBottom(this), ime.getBottom(this))
    return visibleDockletBandPx(barInsets.getBottom(this), systemBottom, hiddenFraction)
}
