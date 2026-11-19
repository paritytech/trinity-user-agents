package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.getSystemService
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner

// Normalized device tilt for the card shine:
// both axes are -1..1 relative to a neutral hold captured after a short warmup, so "centre" matches
// however the user naturally holds the phone.
@Immutable
data class TiltState(
    val x: Float,
    val y: Float
)

// Sensor frames skipped before the neutral reference is captured, so it locks onto the user's hold.
private const val WARMUP_FRAMES = 15

// Gravity delta (in g) that maps to the full ±1 tilt range; beyond it the tilt clamps.
private const val SENSITIVITY = 0.30f

// Per-event lerp toward the latest reading; larger = snappier.
private const val SMOOTHING = 0.55f

// Static neutral tilt, used when no provider is present (e.g. @Preview) so cards still render.
private val ZeroTilt: State<TiltState> = mutableStateOf(TiltState(0f, 0f))

// Screen-scoped device tilt, shared by the list + details cards through composition. PocketScreen
// provides the single live source so both cards read the same tilt and stay continuous across the
// shared-element transition. Read it inside draw lambdas to keep updates off the recomposition path.
val LocalCardTilt = compositionLocalOf { ZeroTilt }

// Creates the live tilt source, owned by the calling composition. Reads the device gravity vector,
// captures a neutral reference after a short warmup and smooths the normalized delta.
@Composable
fun rememberCardTilt(): State<TiltState> {
    val context = LocalContext.current.applicationContext
    val lifecycleOwner = LocalLifecycleOwner.current
    val source = remember { TiltSource() }

    // Sensor lives with the Pocket screen's composition: MainScreen swaps tabs via Crossfade, so this
    // composable is present only while the WALLET tab is selected. Start on enter / stop on leave ties the
    // sensor to that visibility; the lifecycle observer only pauses it while the app is backgrounded.
    DisposableEffect(lifecycleOwner, source) {
        source.start(context)

        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> source.start(context)
                Lifecycle.Event.ON_STOP -> source.stop()
                else -> Unit
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)

        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
            source.stop()
        }
    }

    return source.tilt
}

// One owner per composition (no ref-count): the sensor is registered while the screen is started and
// unregistered when it stops or the holder leaves composition.
private class TiltSource {
    val tilt = mutableStateOf(TiltState(0f, 0f))

    // Smoothed on the sensor thread; tilt state is a snapshot write so reads stay consistent.
    private var tiltX = 0f
    private var tiltY = 0f

    private var referenceX = 0f
    private var referenceY = 0f
    private var hasReference = false
    private var warmupFrames = 0

    private var sensorManager: SensorManager? = null
    private var listener: SensorEventListener? = null

    fun start(context: Context) {
        if (listener != null) return

        val manager = context.getSystemService<SensorManager>() ?: return
        val sensor = manager.getDefaultSensor(Sensor.TYPE_GRAVITY)
            ?: manager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
            ?: return
        sensorManager = manager

        hasReference = false
        warmupFrames = 0
        tiltX = 0f
        tiltY = 0f
        tilt.value = TiltState(0f, 0f)

        listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) = onSensorEvent(event)
            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
        }
        manager.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
    }

    fun stop() {
        listener?.let { sensorManager?.unregisterListener(it) }
        sensorManager = null
        listener = null
    }

    private fun onSensorEvent(event: SensorEvent) {
        // Android's gravity sensor reports m/s² pointing away from the ground; Core Motion reports g
        // pointing toward it. Negate + normalize.
        val gx = -event.values[0] / SensorManager.GRAVITY_EARTH
        val gy = -event.values[1] / SensorManager.GRAVITY_EARTH

        if (!hasReference) {
            warmupFrames++
            if (warmupFrames >= WARMUP_FRAMES) {
                referenceX = gx
                referenceY = gy
                hasReference = true
            }
            return
        }

        val targetX = ((gx - referenceX) / SENSITIVITY).coerceIn(-1f, 1f)
        val targetY = ((referenceY - gy) / SENSITIVITY).coerceIn(-1f, 1f)

        tiltX += (targetX - tiltX) * SMOOTHING
        tiltY += (targetY - tiltY) * SMOOTHING

        tilt.value = TiltState(x = tiltX, y = tiltY)
    }
}
