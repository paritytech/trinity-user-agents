package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import android.graphics.Paint
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

// The ring takes the next mode's colours once it is nearer to that mode than the one it left, cross-faded
// rather than swapped, since a swap in a single frame reads as a jump. [fadeMillis] is read when a crossing
// starts, so a fast drag gets a short fade.
@Composable
internal fun SelectionRing(
    modifier: Modifier,
    colors: CircleColors,
    fadeMillis: () -> Int
) {
    // A crossing that interrupts an unfinished fade continues from the colours on screen: a two-step tap slide
    // crosses two midpoints inside one fade, and restarting from the first mode would snap the ring back.
    var fadeFrom by remember { mutableStateOf(colors) }
    var fadeTo by remember { mutableStateOf(colors) }
    val fade = remember { Animatable(1f) }

    LaunchedEffect(colors) {
        if (fadeTo == colors) return@LaunchedEffect

        fadeFrom = fadeFrom.blendedTo(fadeTo, fade.value)
        fadeTo = colors
        fade.snapTo(0f)
        fade.animateTo(1f, tween(durationMillis = fadeMillis(), easing = LinearEasing))
    }

    val blended = if (fade.value >= 1f) fadeTo else fadeFrom.blendedTo(fadeTo, fade.value)

    val shadowColor = PolkadotTheme.colors.shadow.medium
    val density = LocalDensity.current
    val shadowBlur = rememberBlurMaskFilter(SHADOW_BLUR)
    val shadowPaint = remember(shadowColor, shadowBlur, density) {
        Paint().apply {
            isAntiAlias = true
            style = Paint.Style.STROKE
            strokeWidth = with(density) { RING_STROKE.toPx() }
            color = shadowColor.copy(alpha = SELECTED_SHADOW_ALPHA).toArgb()
            maskFilter = shadowBlur
        }
    }
    val stroke = remember(density) { with(density) { Stroke(width = RING_STROKE.toPx()) } }
    val brush = remember(blended, density) {
        with(density) {
            Brush.verticalGradient(
                colors = listOf(blended.rimTop, blended.rimBottom),
                startY = RING_TOP_INSET.toPx(),
                endY = (RING_TOP_INSET + RING_OUTER_SIZE - RING_STROKE).toPx()
            )
        }
    }

    Spacer(
        // The caller's modifier carries the offset that puts the ring on its mode, so it has to sit before the
        // drawing: a draw modifier paints at the node's own position, ignoring a later shift.
        modifier = Modifier
            .size(MODE_BOX_SIZE)
            .then(modifier)
            .drawWithCache {
                val centreX = size.width / 2f
                val centreY = size.height / 2f
                val half = (RING_OUTER_SIZE - RING_STROKE).toPx() / 2f
                val corner = (RING_CORNER_RADIUS - RING_STROKE / 2).toPx()
                val shadowOffset = SHADOW_OFFSET.toPx()
                val topLeft = Offset(centreX - half, centreY - half)
                val ringSize = Size(half * 2f, half * 2f)

                onDrawBehind {
                    drawIntoCanvas { canvas ->
                        canvas.nativeCanvas.drawRoundRect(
                            centreX - half,
                            centreY - half + shadowOffset,
                            centreX + half,
                            centreY + half + shadowOffset,
                            corner,
                            corner,
                            shadowPaint
                        )
                    }

                    drawRoundRect(
                        brush = brush,
                        topLeft = topLeft,
                        size = ringSize,
                        cornerRadius = CornerRadius(corner),
                        style = stroke
                    )
                }
            }
    )
}

private val RING_OUTER_SIZE = 62.dp
private val RING_CORNER_RADIUS = 24.dp
private val RING_STROKE = 2.dp

internal val MODE_BOX_SIZE = RING_OUTER_SIZE + (SHADOW_OFFSET + SHADOW_BLUR) * 2

// The ring's gradient spans the ring, not the box it sits in.
private val RING_TOP_INSET = (MODE_BOX_SIZE - RING_OUTER_SIZE + RING_STROKE) / 2
