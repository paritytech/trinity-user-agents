package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.view.Surface
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.exp
import kotlin.math.roundToInt
import kotlin.math.sqrt

/** The parallax rules, kept apart from the sensor and the layout so they can be checked on their own. */
internal object ParallaxRules {
    /**
     * The node's move, in hundredths of a dp along the layout direction and down, for a [tilt] from the rest attitude
     * in degrees (end edge lowered, top edge lowered) and the pan's children drawn at [pan]: the core's
     * `Parallax::offset`.
     */
    fun offset(parallax: JsModifier.Parallax, tilt: Offset, pan: Offset): Offset {
        val follow = parallax.tilt
        val tilted = if (follow != null && follow.maxTiltCentideg > 0) {
            val max = follow.maxTiltCentideg / HUNDREDTHS
            fun along(degrees: Float) = follow.travel * (degrees / max).coerceIn(-1f, 1f)
            Offset(along(tilt.x), along(tilt.y))
        } else {
            Offset.Zero
        }
        val panned = if (parallax.pan != null) pan else Offset.Zero
        return (tilted + panned) * (parallax.depthPermille / PERMILLE)
    }

    /**
     * Degrees of tilt from a gravity sensor's reading (x towards the screen's right edge, y towards its top, z out of
     * it, pointing away from the ground): across, the right edge lowered; down, the top edge turned towards the ground.
     */
    fun degrees(x: Float, y: Float, z: Float): Offset? {
        val length = sqrt(x * x + y * y + z * z)
        if (length <= 0f || !length.isFinite()) return null
        val across = asin((-x / length).coerceIn(-1f, 1f))
        val down = atan2(-y / length, z / length)
        return Offset(Math.toDegrees(across.toDouble()).toFloat(), Math.toDegrees(down.toDouble()).toFloat())
    }

    /** A device-frame reading turned into the screen's frame for a display [rotation] (`Surface.ROTATION_*`). */
    fun toScreen(x: Float, y: Float, rotation: Int): Pair<Float, Float> = when (rotation) {
        Surface.ROTATION_90 -> -y to x
        Surface.ROTATION_180 -> -x to -y
        Surface.ROTATION_270 -> y to -x
        else -> x to y
    }

    private const val HUNDREDTHS = 100f
    private const val PERMILLE = 1000f
}

/**
 * The tilt measured from a rest attitude: the device's at the first reading, which then follows the device's own on a
 * time constant (zero keeps it), so the node drifts home when the device is held still another way.
 */
internal class TiltRest {
    private var rest: Offset? = null
    private var lastNanos: Long? = null

    fun tilt(current: Offset, atNanos: Long, settleMs: Int): Offset {
        val before = rest
        val elapsedMs = lastNanos?.let { (atNanos - it).coerceAtLeast(0L) / NANOS_PER_MS } ?: 0.0
        lastNanos = atNanos
        if (before == null) {
            rest = current
            return Offset.Zero
        }
        val moved = if (settleMs > 0) before + (current - before) * (1 - exp(-elapsedMs / settleMs)).toFloat() else before
        rest = moved
        return current - moved
    }
}

/** A gravity reading in degrees (see [ParallaxRules.degrees]) and when it was taken. */
internal class TiltReading(val degrees: Offset, val nanos: Long)

/**
 * Moves the node by its parallax after layout, as a translate does. The pan's part follows the keyed gesture's children
 * wherever they are drawn; the tilt's part adds nothing where animations are off or the device has no gravity to read.
 */
@Composable
internal fun JsModifier.Parallax.parallaxModifier(): Modifier {
    val parallax = this
    val place = pan?.let { LocalGesturePanPlaces.current.place(it) }
    val enabled = LocalRendererMotionEnabled.current ?: rendererMotionEnabled()
    val follow = tilt?.takeIf { enabled && it.maxTiltCentideg > 0 }
    val reading = rememberDeviceTilt(follow != null)
    val rest = remember { TiltRest() }
    val density = LocalDensity.current.density
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    return Modifier.offset {
        val tilted = follow?.let { tilt -> reading.value?.let { rest.tilt(it.degrees, it.nanos, tilt.settleMs) } } ?: Offset.Zero
        // Lowering the end edge moves the node towards the end: the left edge, right to left.
        val alongLayout = Offset(if (rtl) -tilted.x else tilted.x, tilted.y)
        val drawn = place?.offset?.invoke() ?: Offset.Zero
        val move = ParallaxRules.offset(parallax, alongLayout, drawn / density * HUNDREDTHS) / HUNDREDTHS * density
        IntOffset(move.x.roundToInt(), move.y.roundToInt())
    }
}

/** The device's gravity in degrees while [follows], read on the main thread and released when it stops. */
@Composable
private fun rememberDeviceTilt(follows: Boolean): State<TiltReading?> {
    val reading = remember { mutableStateOf<TiltReading?>(null) }
    val context = LocalContext.current
    val view = LocalView.current
    DisposableEffect(follows, context) {
        val sensors = context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
        val sensor = sensors?.getDefaultSensor(Sensor.TYPE_GRAVITY) ?: sensors?.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                val (x, y) = ParallaxRules.toScreen(event.values[0], event.values[1], view.display?.rotation ?: Surface.ROTATION_0)
                ParallaxRules.degrees(x, y, event.values[2])?.let { reading.value = TiltReading(it, event.timestamp) }
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
        }
        if (follows && sensor != null) sensors?.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
        onDispose {
            sensors?.unregisterListener(listener)
            reading.value = null
        }
    }
    return reading
}

private const val HUNDREDTHS = 100f
private const val NANOS_PER_MS = 1_000_000.0
