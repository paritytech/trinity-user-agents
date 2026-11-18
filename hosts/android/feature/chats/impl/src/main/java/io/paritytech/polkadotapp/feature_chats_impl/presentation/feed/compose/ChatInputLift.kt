package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.visibleDockletBandPx

internal fun Modifier.liftAboveDocklet(
    barInsets: WindowInsets,
    navigationBars: WindowInsets,
    ime: WindowInsets,
    hiddenFraction: () -> Float,
): Modifier = layout { measurable, constraints ->
    val systemBottom = maxOf(navigationBars.getBottom(this), ime.getBottom(this))
    val lift = visibleDockletBandPx(barInsets.getBottom(this), systemBottom, hiddenFraction())
    val placeable = measurable.measure(constraints.copy(minHeight = 0))
    val height = (placeable.height + lift).coerceIn(constraints.minHeight, constraints.maxHeight)
    layout(placeable.width, height) { placeable.place(0, 0) }
}
