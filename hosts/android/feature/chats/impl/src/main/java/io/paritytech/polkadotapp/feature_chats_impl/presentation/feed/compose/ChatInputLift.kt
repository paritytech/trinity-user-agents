package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import kotlin.math.roundToInt

internal fun chatInputLiftPx(barInsetBottom: Int, systemBottom: Int, hiddenFraction: Float): Int =
    ((barInsetBottom - systemBottom).coerceAtLeast(0) * (1f - hiddenFraction)).roundToInt()

internal fun Modifier.shrinkAboveDocklet(
    barInsets: WindowInsets,
    navigationBars: WindowInsets,
    ime: WindowInsets,
    hiddenFraction: () -> Float,
): Modifier = layout { measurable, constraints ->
    val systemBottom = maxOf(navigationBars.getBottom(this), ime.getBottom(this))
    val lift = chatInputLiftPx(barInsets.getBottom(this), systemBottom, hiddenFraction())
    val placeable = measurable.measure(
        constraints.copy(minHeight = 0, maxHeight = (constraints.maxHeight - lift).coerceAtLeast(0))
    )
    layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(0, 0) }
}
