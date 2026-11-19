package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler

// The game rotation vector has no heading, so readings from it carry none.
internal class DeviceOrientationSensor(
    context: Context,
    private val handler: Handler,
    private val onReading: (rotation: FloatArray, hasHeading: Boolean) -> Unit,
) {
    private val manager = context.getSystemService(SensorManager::class.java)
    private val sensor = manager?.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR)
        ?: manager?.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
    private val hasHeading = sensor?.type == Sensor.TYPE_ROTATION_VECTOR
    private val rotation = FloatArray(ROTATION_ENTRIES)

    var listening = false
        private set

    private val listener = object : SensorEventListener {
        override fun onSensorChanged(event: SensorEvent) {
            SensorManager.getRotationMatrixFromVector(rotation, event.values)
            onReading(rotation, hasHeading)
        }

        override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
    }

    fun start() {
        if (listening || manager == null || sensor == null) return
        listening = manager.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME, handler)
    }

    fun stop() {
        if (!listening) return
        manager?.unregisterListener(listener)
        listening = false
    }

    private companion object {
        const val ROTATION_ENTRIES = 9
    }
}
