package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import android.graphics.RuntimeShader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas

private val MOTION_SHINE_AGSL = """
uniform float2 size;
uniform float2 tilt;
uniform float strength;
uniform float dimming;
uniform float2 spread;
uniform float center;

half4 main(float2 position) {
    float2 centre = size * 0.5;

    const float2 dir  = float2(${ShineMainAxis.x}, ${ShineMainAxis.y});
    const float2 perp = float2(${ShinePerpAxis.x}, ${ShinePerpAxis.y});

    float halfMain = 0.5 * (abs(dir.x) * size.x + abs(dir.y) * size.y);
    float t = 0.5 + dot(position - centre, dir) / (2.0 * halfMain);
    t += tilt.x * $SHINE_MAIN_TILT_X - tilt.y * $SHINE_MAIN_TILT_Y;

    float halfPerp = 0.5 * (abs(perp.x) * size.x + abs(perp.y) * size.y);
    float s = 0.5 + dot(position - centre, perp) / (2.0 * halfPerp);
    s += tilt.y * $SHINE_PERP_TILT_Y - tilt.x * $SHINE_PERP_TILT_X;

    float dMain = (t - center) / spread.x;
    float dPerp = (s - 0.5) / spread.y;
    float falloff = dMain * dMain + dPerp * dPerp;
    float core = exp(-falloff);
    float glow = exp(-falloff / 4.0);

    float shineAlpha = core * strength;
    float dimAlpha = (1.0 - glow) * dimming;

    half white = half(shineAlpha);
    half a = half(clamp(shineAlpha + dimAlpha, 0.0, 1.0));
    return half4(white, white, white, a);
}
"""

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
@Composable
internal fun Modifier.motionShineShader(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters
): Modifier {
    val shader = remember { RuntimeShader(MOTION_SHINE_AGSL) }
    val brush = remember(shader) { ShaderBrush(shader) }

    return drawWithCache {
        shader.applyParameters(size.width, size.height, parameters)
        onDrawBehind {
            val tilt = tiltState.value
            shader.setFloatUniform("tilt", tilt.x, tilt.y)
            drawRect(brush)
        }
    }
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
@Composable
internal fun Modifier.maskedMotionShineShader(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters,
    contentAlpha: Float
): Modifier {
    val shader = remember { RuntimeShader(MOTION_SHINE_AGSL) }
    val brush = remember(shader) { ShaderBrush(shader) }

    return drawWithCache {
        shader.applyParameters(size.width, size.height, parameters)
        onDrawWithContent {
            val tilt = tiltState.value
            shader.setFloatUniform("tilt", tilt.x, tilt.y)
            val layerRect = Rect(Offset.Zero, size)

            drawIntoCanvas { canvas ->
                canvas.saveLayer(layerRect, Paint().apply { alpha = contentAlpha })
                drawContent()
                canvas.restore()

                canvas.saveLayer(layerRect, Paint())
                drawContent()
                drawRect(brush, blendMode = BlendMode.SrcIn)
                canvas.restore()
            }
        }
    }
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private fun RuntimeShader.applyParameters(width: Float, height: Float, parameters: MotionShineParameters) {
    setFloatUniform("size", width, height)
    setFloatUniform("strength", parameters.intensity)
    setFloatUniform("dimming", parameters.dimming)
    setFloatUniform("spread", parameters.width, parameters.length)
    setFloatUniform("center", parameters.center)
}
