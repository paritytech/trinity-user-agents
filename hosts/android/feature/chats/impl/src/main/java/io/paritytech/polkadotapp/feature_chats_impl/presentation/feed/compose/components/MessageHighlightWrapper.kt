package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private const val HIGHLIGHT_FADE_IN_MS = 250
private const val HIGHLIGHT_FADE_OUT_MS = 800
internal const val HIGHLIGHT_PULSE_DURATION_MS = (HIGHLIGHT_FADE_IN_MS + HIGHLIGHT_FADE_OUT_MS).toLong()

@Composable
internal fun MessageHighlightWrapper(
    isHighlighted: Boolean,
    content: @Composable () -> Unit
) {
    val highlightAlpha = remember { Animatable(0f) }

    LaunchedEffect(isHighlighted) {
        if (isHighlighted) {
            highlightAlpha.animateTo(
                targetValue = 0.3f,
                animationSpec = tween(durationMillis = HIGHLIGHT_FADE_IN_MS)
            )
            highlightAlpha.animateTo(
                targetValue = 0f,
                animationSpec = tween(durationMillis = HIGHLIGHT_FADE_OUT_MS)
            )
        }
    }

    Box {
        content()

        if (highlightAlpha.value > 0f) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .clip(PolkadotTheme.shapes.mediumIncreased)
                    .background(Color.White.copy(alpha = highlightAlpha.value))
            )
        }
    }
}
