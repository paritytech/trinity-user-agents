package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import android.graphics.BlurMaskFilter
import android.graphics.Paint
import androidx.compose.foundation.layout.Spacer
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import android.graphics.Path as AndroidPath

// The recessed groove the modes sit in, carrying the continuous speed-to-privacy scale as tick marks.
// Drawn rather than composed: an inner shadow has no Compose primitive, and `Modifier.blur` is API 31+ while
// the app ships from 29 — so it is drawn as a blurred complement clipped to the groove, the same
// `BlurMaskFilter` approach the design system already uses for blurred mnemonics.
@Composable
internal fun PrivacyModeTrack(modifier: Modifier) {
    // The floor is the card's own surface taken a shade down, not a darker token: `main` is the screen
    // behind the card and reads as a hole punched through it, while `nested` is *lighter* than the card and
    // turns the recess into a ridge. The design puts #14151B on a #1A1B20 card — six units, which is the
    // fill's entire contribution. Everything that reads as depth is the shadow below it.
    val troughColor = lerp(
        PolkadotTheme.colors.bg.surface.container,
        PolkadotTheme.colors.bg.surface.main,
        TROUGH_SHADE
    )
    // Black at 48%, which is what the design's two inner shadows — 45% and 50% — both round to.
    val recessColor = PolkadotTheme.colors.shadow.medium.softenedOn(troughColor)
    // The lip the groove is cut into catches light along its top and bottom edges. A surface shade rather
    // than a stroke token: the design's edge is #2A2B30, and every stroke token is far brighter than that.
    val rimColor = PolkadotTheme.colors.bg.surface.nested

    val scaleColors = listOf(
        PolkadotTheme.colors.bg.status.warning,
        PolkadotTheme.colors.bg.status.success,
        PolkadotTheme.colors.avatar.bg.sapphire,
        PolkadotTheme.colors.avatar.bg.amethyst
    )

    Spacer(
        modifier = Modifier.drawWithCache {
            val cornerRadius = size.height / 2f
            val troughRect = size.toRect()
            val trough = roundRectPath(troughRect, cornerRadius)

            val rimOffset = RIM_OFFSET.toPx()
            val rimAbove = roundRectPath(troughRect.translate(Offset(0f, -rimOffset)), cornerRadius)
            val rimBelow = roundRectPath(troughRect.translate(Offset(0f, rimOffset)), cornerRadius)

            // The scale spans centre-to-centre of the outer modes, so its ends disappear under them rather
            // than stopping short of the groove's rounded caps.
            val scaleStart = TRACK_INSET.toPx()
            val scaleEnd = size.width - TRACK_INSET.toPx()
            val ticks = buildTicks(
                startX = scaleStart,
                endX = scaleEnd,
                centreY = size.height / 2f,
                tickWidth = TICK_WIDTH.toPx(),
                tickHeight = TICK_HEIGHT.toPx(),
                step = TICK_STEP.toPx()
            )
            val scaleBrush = Brush.horizontalGradient(
                colorStops = arrayOf(
                    0f to scaleColors[0],
                    SCALE_BALANCED_STOP to scaleColors[1],
                    SCALE_SAPPHIRE_STOP to scaleColors[2],
                    1f to scaleColors[3]
                ),
                startX = scaleStart,
                endX = scaleEnd
            )

            val recess = listOf(
                RECESS_SHALLOW_OFFSET to RECESS_SHALLOW_BLUR,
                RECESS_DEEP_OFFSET to RECESS_DEEP_BLUR
            ).map { (offset, blur) ->
                innerShadow(troughRect, cornerRadius, offset.toPx(), blur.toPx(), recessColor)
            }
            val clip = trough.asAndroidPath()

            onDrawBehind {
                // Two copies of the groove a pixel above and below it, covered by the groove itself: what
                // stays visible is the lit edge of the lip, and only where the light would reach it.
                drawPath(rimAbove, color = rimColor)
                drawPath(rimBelow, color = rimColor)
                drawPath(trough, color = troughColor)

                drawIntoCanvas { canvas ->
                    val native = canvas.nativeCanvas
                    val checkpoint = native.save()
                    native.clipPath(clip)

                    for ((path, paint) in recess) {
                        native.drawPath(path, paint)
                    }

                    native.restoreToCount(checkpoint)
                }

                drawPath(ticks, brush = scaleBrush)
            }
        }.then(modifier)
    )
}

// The design exists only in the dark theme, where the palette's shadow — black at 48%, the same value in
// every theme — falls on a #14151B floor and is a whisper. Contrast is an absolute difference though, so on
// the light palettes' near-white floor that alpha is a bruise: heavy enough that the groove reads as
// top-weighted and the modes sitting in it as sitting low. Scaling by what the surface has left to give
// keeps the shadow exactly as drawn wherever the floor is dark, and lets it fall away as the floor brightens.
private fun Color.softenedOn(surface: Color): Color =
    copy(alpha = alpha * (1f - surface.luminance() * LIGHT_SURFACE_FALLOFF))

private fun roundRectPath(rect: Rect, cornerRadius: Float): Path = Path().apply {
    addRoundRect(RoundRect(rect, CornerRadius(cornerRadius)))
}

// The groove's own complement, shifted down by [offset] and blurred. Drawn under a clip to the groove, what
// survives is the band its upper rim shadows — an inner shadow, which Compose has no primitive for.
// The outer bound only has to clear the blur, since the clip discards everything past the groove anyway.
private fun innerShadow(
    rect: Rect,
    cornerRadius: Float,
    offset: Float,
    blur: Float,
    color: Color
): Pair<AndroidPath, Paint> {
    val path = Path().apply {
        addRect(rect.inflate(offset + blur * 2f))
        addRoundRect(RoundRect(rect.translate(Offset(0f, offset)), CornerRadius(cornerRadius)))
        fillType = PathFillType.EvenOdd
    }
    val paint = Paint().apply {
        isAntiAlias = true
        this.color = color.toArgb()
        maskFilter = BlurMaskFilter(blur, BlurMaskFilter.Blur.NORMAL)
    }

    return path.asAndroidPath() to paint
}

// One upright bar per step, laid left to right; the gradient brush colours them by position.
private fun buildTicks(
    startX: Float,
    endX: Float,
    centreY: Float,
    tickWidth: Float,
    tickHeight: Float,
    step: Float
): Path {
    val path = Path()
    val top = centreY - tickHeight / 2f
    var x = startX

    while (x + tickWidth <= endX) {
        path.addRect(Rect(offset = Offset(x, top), size = Size(tickWidth, tickHeight)))
        x += step
    }

    return path
}

internal val TRACK_HEIGHT = 40.dp

// The outer modes are centred on the groove's end caps.
internal val TRACK_INSET = TRACK_HEIGHT / 2

private val TICK_WIDTH = 2.dp
private val TICK_HEIGHT = 6.dp

// Also the haptic grain of a drag: the selector ticks once per mark the ring passes.
internal val TICK_STEP = 8.dp

private const val SCALE_BALANCED_STOP = 0.49f
private const val SCALE_SAPPHIRE_STOP = 0.73f

/** How far the floor sits below the card it is cut into: #1A1B20 towards #0B0C0F lands on the design's #14151B. */
private const val TROUGH_SHADE = 0.4f

/** How much of the shadow a fully lit surface takes away; a dark one keeps nearly all of it. */
private const val LIGHT_SURFACE_FALLOFF = 0.8f

private val RIM_OFFSET = 1.dp

private val RECESS_SHALLOW_OFFSET = 3.dp
private val RECESS_SHALLOW_BLUR = 6.dp

private val RECESS_DEEP_OFFSET = 7.dp
private val RECESS_DEEP_BLUR = 5.dp
