package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.StartOffset
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.rememberReducedMotion

private val DotSize = 4.dp
private const val DOT_COUNT = 3
private const val DOT_CYCLE_MS = 600
private const val DOT_DELAY_MS = 200
private const val DIM_ALPHA = 0.3f

/**
 * Three dots that show an activity that continues, for example "Bot is typing". When the user removed
 * animations in the system settings, the dots do not move.
 */
@Composable
internal fun OngoingActivityIndicator(
    modifier: Modifier = Modifier,
    color: Color = PolkadotTheme.colors.fg.secondary,
) {
    val reducedMotion = rememberReducedMotion()

    Row(
        modifier = modifier.clearAndSetSemantics { },
        horizontalArrangement = Arrangement.spacedBy(2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (reducedMotion) {
            repeat(DOT_COUNT) { Dot(color = color, alpha = 1f) }
        } else {
            val transition = rememberInfiniteTransition(label = "ongoingActivity")

            repeat(DOT_COUNT) { index ->
                val alpha by transition.animateFloat(
                    initialValue = DIM_ALPHA,
                    targetValue = 1f,
                    animationSpec = infiniteRepeatable(
                        animation = tween(DOT_CYCLE_MS, easing = LinearEasing),
                        repeatMode = RepeatMode.Reverse,
                        initialStartOffset = StartOffset(index * DOT_DELAY_MS),
                    ),
                    label = "ongoingActivityDot",
                )
                Dot(color = color, alpha = alpha)
            }
        }
    }
}

@Composable
private fun Dot(color: Color, alpha: Float) {
    Box(
        modifier = Modifier
            .size(DotSize)
            .alpha(alpha)
            .background(color = color, shape = PolkadotTheme.shapes.full)
    )
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun OngoingActivityIndicatorPreview() {
    PolkadotTheme {
        OngoingActivityIndicator()
    }
}
