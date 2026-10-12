package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import kotlin.math.abs
import kotlin.math.exp

private const val PROFILE_STOPS = 25

@Composable
internal fun Modifier.motionShineFallback(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters
): Modifier {
    val shineStops = remember(parameters) { shineProfileStops(parameters) }
    val dimStops = remember(parameters) { dimProfileStops(parameters) }

    return drawBehind {
        val tilt = tiltState.value
        val shift = tilt.x * HOLO_RAMP_TILT_X - tilt.y * HOLO_RAMP_TILT_Y
        drawRect(brush = axisGradient(dimStops, HoloMainAxis, size, shift))
        drawRect(brush = axisGradient(shineStops, HoloMainAxis, size, shift))
    }
}

@Composable
internal fun Modifier.maskedMotionShineFallback(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters,
    contentAlpha: Float
): Modifier {
    val shineStops = remember(parameters) { shineProfileStops(parameters) }
    val dimStops = remember(parameters) { dimProfileStops(parameters) }

    return drawWithContent {
        val tilt = tiltState.value
        val shift = tilt.x * HOLO_RAMP_TILT_X - tilt.y * HOLO_RAMP_TILT_Y
        val layerRect = Rect(Offset.Zero, size)

        drawIntoCanvas { canvas ->
            canvas.saveLayer(layerRect, Paint().apply { alpha = contentAlpha })
            drawContent()
            canvas.restore()

            canvas.saveLayer(layerRect, Paint())
            drawContent()
            canvas.saveLayer(layerRect, Paint().apply { blendMode = BlendMode.SrcIn })
            drawRect(brush = axisGradient(dimStops, HoloMainAxis, size, shift))
            drawRect(brush = axisGradient(shineStops, HoloMainAxis, size, shift))
            canvas.restore()
            canvas.restore()
        }
    }
}

private fun shineProfileStops(parameters: MotionShineParameters): Array<Pair<Float, Color>> =
    profileStops { t ->
        val d = (t - parameters.center) / parameters.width
        Color.White.copy(alpha = (exp(-d * d) * parameters.intensity).coerceIn(0f, 1f))
    }

private fun dimProfileStops(parameters: MotionShineParameters): Array<Pair<Float, Color>> =
    profileStops { t ->
        val d = (t - parameters.center) / parameters.width
        Color.Black.copy(alpha = ((1f - exp(-d * d / 4f)) * parameters.dimming).coerceIn(0f, 1f))
    }

private fun profileStops(colorAt: (Float) -> Color): Array<Pair<Float, Color>> =
    Array(PROFILE_STOPS) { index ->
        val t = index / (PROFILE_STOPS - 1f)
        t to colorAt(t)
    }

private fun axisGradient(
    colorStops: Array<Pair<Float, Color>>,
    axis: Offset,
    size: Size,
    shift: Float
): Brush {
    val centre = Offset(size.width * 0.5f, size.height * 0.5f)
    val halfSpan = 0.5f * (abs(axis.x) * size.width + abs(axis.y) * size.height)
    val translation = axis * (-shift * 2f * halfSpan)
    return Brush.linearGradient(
        colorStops = colorStops,
        start = centre - axis * halfSpan + translation,
        end = centre + axis * halfSpan + translation
    )
}
