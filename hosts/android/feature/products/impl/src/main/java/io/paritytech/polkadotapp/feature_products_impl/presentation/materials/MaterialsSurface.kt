package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import android.animation.ValueAnimator
import android.graphics.Bitmap
import android.graphics.SurfaceTexture
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.view.TextureView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.layer.GraphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.ReadyLayer
import java.util.concurrent.atomic.AtomicReference

@Composable
fun MaterialsSurface(
    modifier: Modifier,
    layers: ImmutableList<ReadyLayer>,
    noiseSeed: UInt,
    base: GraphicsLayer,
    baseDraws: StateFlow<Int>,
    onFrameShown: () -> Unit,
    onFailure: (MaterialDrawFailure) -> Unit,
) {
    val context = LocalContext.current
    val readsMotion = layers.any { it.shader.readsMotion }
    val stilled = remember(context, readsMotion) {
        motionStilled(
            reducedMotion = !ValueAnimator.areAnimatorsEnabled(),
            readsMotion = readsMotion,
            hasMotionSensor = context.getSystemService(SensorManager::class.java)?.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR) != null,
        )
    }
    val motion = rememberDeviceMotion(enabled = readsMotion && !stilled)
    val currentOnFrameShown by rememberUpdatedState(onFrameShown)
    val currentOnFailure by rememberUpdatedState(onFailure)
    var renderer by remember { mutableStateOf<MaterialsGlRenderer?>(null) }

    AndroidView(
        modifier = modifier,
        factory = { viewContext ->
            TextureView(viewContext).apply {
                isOpaque = false
                surfaceTextureListener = object : TextureView.SurfaceTextureListener {
                    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
                        renderer = MaterialsGlRenderer(surface, layers, noiseSeed, motion::get, stilled) { failure -> currentOnFailure(failure) }
                            .also { it.start() }
                    }

                    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) = Unit

                    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
                        val releasedByRenderer = renderer?.handOverSurface() == true
                        renderer = null
                        return !releasedByRenderer
                    }

                    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) = currentOnFrameShown()
                }
            }
        },
    )

    LaunchedEffect(renderer) {
        val current = renderer ?: return@LaunchedEffect
        baseDraws.collect {
            if (base.size.width > 0 && base.size.height > 0) {
                val bitmap = base.toImageBitmap().asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false)
                current.updateBase(bitmap, bitmap.width, bitmap.height)
            }
        }
    }

    DisposableEffect(Unit) {
        onDispose { renderer?.finish() }
    }
}

@Composable
private fun rememberDeviceMotion(enabled: Boolean): AtomicReference<DeviceMotion?> {
    val context = LocalContext.current
    val motion = remember { AtomicReference<DeviceMotion?>(null) }
    DisposableEffect(enabled, context) {
        val sensors = context.getSystemService(SensorManager::class.java)
        val sensor = sensors?.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR)
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                motion.set(event.values.toDeviceMotion())
            }

            override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) = Unit
        }
        if (enabled && sensor != null) sensors.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
        onDispose {
            sensors?.unregisterListener(listener)
            motion.set(null)
        }
    }
    return motion
}

private fun FloatArray.toDeviceMotion(): DeviceMotion {
    val rows = FloatArray(MATRIX_SIZE)
    SensorManager.getRotationMatrixFromVector(rows, this)
    return deviceMotion(rows.toList())
}
