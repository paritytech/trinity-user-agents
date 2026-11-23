package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import kotlin.math.PI

/**
 * The existing slider's look and motion, value for value: its colours, pill, text lines, thumb, chevron, spinner and
 * measured constants. The native slide to confirm draws with these, so it looks as the existing slider does.
 */
object SlideStyle {
    /** `--track` */
    const val TRACK = 0x33363AFFL

    /** `--thumb` */
    const val THUMB = 0xFDFDFDFFL

    /** `--chevron` */
    val chevron = Color(0xFF0A0A0A)

    /** `#thumb` box-shadow: 0 2px 6px rgba(0,0,0,0.36); 0.36 of 255 is 0x5C. */
    val shadow = JsModifier.Shadow(rgba = 0x0000005CL, x = 0f, y = 2f, blur = 6f)

    /** `#text` left and right, from the pill's edges; its gap and padding-top. */
    const val TEXT_START_DP = 84f
    const val TEXT_END_DP = 16f
    const val GAP_DP = 4.6f
    const val TEXT_PADDING_TOP_DP = 1f

    /** `#text` font-size 18px, line-height 22px, letter-spacing 0; `#l1` 400 in `--line1`, `#l2` 600 in `--line2`. */
    val line1Font = JsModifier.Font(size = 18f, lineHeight = 22f, weight = 400, rgba = 0x585A62FFL, letterSpacing = 0f)
    val line2Font = JsModifier.Font(size = 18f, lineHeight = 22f, weight = 600, rgba = 0xF5F6F9FFL, letterSpacing = 0f)

    /** `FADE_RATE`: text opacity = 1 - 1.4 × progress. */
    const val FADE_RATE = 1.4f

    /** The `#l1` band: its 13 gradient stops, 186 wide, 212 dp/s from 70 before to 70 after, then 1 s at rest. */
    val shimmer = JsModifier.Shimmer(
        rgba = 0xFFFFFFFFL,
        stops = listOf(0f, 0.011f, 0.054f, 0.181f, 0.44f, 0.766f, 0.97f, 0.766f, 0.44f, 0.181f, 0.054f, 0.011f, 0f),
        band = 186f,
        lead = 70f,
        speed = 212f,
        restMs = 1000,
    )

    /** `#spin`: one turn in 0.8 s. */
    const val SPIN_MS = 800

    /** `PRESS_SCALE`, `PRESS_MS`, the 150 ms return to size, the spring (18 rad/s, 0.8), `APPROACH_MS`, `POP`, `POP_OFFSET`. */
    val feel = JsPanFeel(
        pressScale = 1.059f,
        pressMs = 133,
        unpressMs = 150,
        springOmega = 18f,
        springDamping = 0.8f,
        approachMs = 117,
        approachReference = 42f,
        pop = listOf(
            1.059f, 1.132f, 1.192f, 1.225f, 1.235f, 1.237f, 1.237f, 1.237f, 1.232f, 1.226f, 1.219f, 1.198f, 1.149f, 1.099f,
            1.050f, 1.023f, 1.005f, 0.991f, 0.981f, 0.977f, 0.976f, 0.977f, 0.983f, 0.988f, 0.990f, 0.994f, 0.998f, 1.0f,
        ),
        popOffset = 3.33f,
    )

    fun textOpacity(progress: Float): Float = WidgetStyleRules.panFade(progress, FADE_RATE)

    /** The existing slider's rule for an accent: a dark chevron on a light thumb, a white one on a dark thumb. */
    fun glyphTint(fill: Color): Color = if (fill.luminance() > LIGHT_THUMB) chevron else Color.White

    private const val LIGHT_THUMB = 0.55f
}

/** `#chev`: the chevron path in the thumb's 56 box, stroked 2.6 with round ends and joins. */
@Composable
internal fun SlideChevron(tint: Color, mirrored: Boolean, modifier: Modifier = Modifier) {
    Canvas(modifier.size(THUMB_BOX)) {
        val unit = size.width / THUMB_BOX_DP
        fun x(value: Float) = (if (mirrored) THUMB_BOX_DP - value else value) * unit
        val path = Path().apply {
            moveTo(x(CHEVRON_X0), CHEVRON_Y0 * unit)
            lineTo(x(CHEVRON_X1), CHEVRON_Y1 * unit)
            lineTo(x(CHEVRON_X0), CHEVRON_Y2 * unit)
        }
        drawPath(path, tint, style = Stroke(width = CHEVRON_STROKE.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
    }
}

/** `#spin circle`: radius 9, stroked 2.4 with round ends, dashed 34 on from three o'clock, turning once in 0.8 s. */
@Composable
internal fun SlideSpinner(tint: Color, modifier: Modifier = Modifier) {
    val time = rememberFrameMillis()
    Canvas(modifier.size(SPINNER_BOX)) {
        val turn = (time.value % SlideStyle.SPIN_MS) / SlideStyle.SPIN_MS.toFloat() * FULL_TURN
        val radius = SPINNER_RADIUS.dp.toPx()
        val sweep = SPINNER_DASH / (2 * PI.toFloat() * SPINNER_RADIUS) * FULL_TURN
        rotate(turn) {
            drawArc(
                color = tint,
                startAngle = 0f,
                sweepAngle = sweep,
                useCenter = false,
                topLeft = Offset(center.x - radius, center.y - radius),
                size = Size(radius * 2, radius * 2),
                style = Stroke(width = SPINNER_STROKE.dp.toPx(), cap = StrokeCap.Round),
            )
        }
    }
}

private const val THUMB_BOX_DP = 56f
private val THUMB_BOX = THUMB_BOX_DP.dp
private const val CHEVRON_X0 = 24.63f
private const val CHEVRON_X1 = 31.37f
private const val CHEVRON_Y0 = 21.47f
private const val CHEVRON_Y1 = 28f
private const val CHEVRON_Y2 = 34.53f
private const val CHEVRON_STROKE = 2.6f
private const val SPINNER_RADIUS = 9f
private const val SPINNER_STROKE = 2.4f
private const val SPINNER_DASH = 34f
private val SPINNER_BOX = 24.dp
private const val FULL_TURN = 360f
