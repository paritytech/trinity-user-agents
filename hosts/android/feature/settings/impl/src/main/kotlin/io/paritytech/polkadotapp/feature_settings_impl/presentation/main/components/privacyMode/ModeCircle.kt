package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import android.graphics.Paint
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import androidx.compose.ui.util.lerp as lerpFloat

// Selecting a mode grows its circle; the mode losing the selection shrinks back in the same motion, so the
// selection reads as passing between two circles while the ring travels.
// The glow is what says a mode has been settled on rather than merely passed over, so it is [CircleState]'s to
// decide and not the size alone: a circle under a moving ring is grown but unlit.
// The box reserves row height for the ring and its shadow and sizes the touch row; the glow may spill past it.
@Composable
internal fun ModeCircle(
    modifier: Modifier,
    appearance: ModeAppearance,
    state: CircleState,
    interactionSource: MutableInteractionSource
) {
    val selection by animateFloatAsState(
        targetValue = if (state == CircleState.Resting) 0f else 1f,
        animationSpec = SELECTION_ANIMATION,
        label = "circleSelection"
    )
    val glow by animateFloatAsState(
        targetValue = if (state == CircleState.Settled) 1f else 0f,
        animationSpec = SELECTION_ANIMATION,
        label = "circleGlow"
    )

    val diameter = lerp(CIRCLE_SIZE, SELECTED_CIRCLE_SIZE, selection)
    val glowColor = appearance.colors.glow
    val shadowColor = PolkadotTheme.colors.shadow.medium
    val shadowAlpha = lerpFloat(UNSELECTED_SHADOW_ALPHA, SELECTED_SHADOW_ALPHA, selection)

    // Held across frames and reconfigured in place: the circle's size animates, which rebuilds the draw
    // cache on every frame of a selection change, so anything created there is created per frame.
    val shadowBlur = rememberBlurMaskFilter(SHADOW_BLUR)
    val glowBlur = rememberBlurMaskFilter(GLOW_BLUR)
    val shadowPaint = remember(shadowBlur) {
        Paint().apply {
            isAntiAlias = true
            maskFilter = shadowBlur
        }
    }
    val glowPaint = remember(glowBlur) {
        Paint().apply {
            isAntiAlias = true
            maskFilter = glowBlur
        }
    }

    Box(
        modifier = Modifier
            .size(MODE_BOX_SIZE)
            .then(modifier),
        contentAlignment = Alignment.Center
    ) {
        Spacer(
            modifier = Modifier
                .size(diameter)
                .drawWithCache {
                    val radius = size.minDimension / 2f
                    val centreX = size.width / 2f
                    val centreY = size.height / 2f
                    val shadowOffset = SHADOW_OFFSET.toPx()
                    val glowHalf = appearance.glowGeometry.size.toPx() / 2f
                    val glowCorner = appearance.glowGeometry.cornerRadius.toPx()

                    shadowPaint.color = shadowColor.copy(alpha = shadowAlpha).toArgb()
                    glowPaint.color = glowColor.copy(alpha = glow * GLOW_ALPHA).toArgb()

                    onDrawBehind {
                        drawIntoCanvas { canvas ->
                            val native = canvas.nativeCanvas

                            if (glow > 0f) {
                                native.drawRoundRect(
                                    centreX - glowHalf,
                                    centreY - glowHalf,
                                    centreX + glowHalf,
                                    centreY + glowHalf,
                                    glowCorner,
                                    glowCorner,
                                    glowPaint
                                )
                            }

                            native.drawCircle(centreX, centreY + shadowOffset, radius, shadowPaint)
                        }
                    }
                }
        )

        PolkadotSurface(
            modifier = Modifier.size(diameter),
            shape = CircleShape,
            brush = appearance.circleBrush(selection),
            border = BorderStroke(CIRCLE_BORDER, appearance.circleBorderBrush(selection))
        ) {
            // PolkadotSurface propagates its minimum constraints, so a size on the icon itself would be
            // clamped straight back up to the circle. This Box absorbs the minimum and lets the inset stand.
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .indication(interactionSource, LocalIndication.current),
                contentAlignment = Alignment.Center
            ) {
                NovaIcon(
                    modifier = Modifier.size(diameter * MODE_ICON_SIZE_FRACTION),
                    imageVector = appearance.icon,
                    tint = PrivacyModeColors.Glyph
                )
            }
        }
    }
}

internal enum class CircleState { Resting, Grown, Settled }

private val CIRCLE_SIZE = 28.dp
private val SELECTED_CIRCLE_SIZE = 50.dp

private val CIRCLE_BORDER = 1.dp

// Figma blurs the glow with a Gaussian sigma of ~12dp; BlurMaskFilter's radius is ~1.7x the sigma it yields.
private val GLOW_BLUR = 20.dp
private const val GLOW_ALPHA = 0.5f

internal val SHADOW_BLUR = 4.dp
internal val SHADOW_OFFSET = 4.dp

// The palette's shadow colour is black at 48%; the design asks for these depths, with nothing darker in the
// set to reach for, so the token supplies the colour and the alphas supply the depth the design drew.
internal const val SELECTED_SHADOW_ALPHA = 0.7f
private const val UNSELECTED_SHADOW_ALPHA = 0.5f

// Share of the circle the glyph takes up; the rest is the inset around it.
private const val MODE_ICON_SIZE_FRACTION = 0.64f

internal val SELECTION_ANIMATION = tween<Float>(durationMillis = 200, easing = FastOutSlowInEasing)
