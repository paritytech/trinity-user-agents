package io.paritytech.polkadotapp.app.root.presentation.debug

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import timber.log.Timber
import kotlin.math.abs

private const val SHAKE_THRESHOLD = 1000f
private const val SHAKE_INTERVAL_MILLIS = 100L
private const val SHAKE_SPEED_MULTIPLIER = 6_000f

class ShakeDetector(context: Context) : SensorEventListener {
    private val sensorManager = context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
    private val accelerometer = sensorManager?.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)

    private var shakeListener: (() -> Unit)? = null

    private var lastShakeTime = 0L
    private var lastX = 0f
    private var lastY = 0f
    private var lastZ = 0f

    init {
        if (accelerometer == null) {
            Timber.w("Device does not have an accelerometer")
        }
    }

    override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) = Unit

    override fun onSensorChanged(event: SensorEvent) {
        val x = event.values[0]
        val y = event.values[1]
        val z = event.values[2]

        val currentTime = System.currentTimeMillis()
        val diffTime = currentTime - lastShakeTime

        if (diffTime < SHAKE_INTERVAL_MILLIS) return

        lastShakeTime = currentTime

        val speed = abs(x + y + z - lastX - lastY - lastZ) / diffTime * SHAKE_SPEED_MULTIPLIER

        lastX = x
        lastY = y
        lastZ = z

        if (speed > SHAKE_THRESHOLD) {
            shakeListener?.invoke()
        }
    }

    fun register(listener: () -> Unit) {
        shakeListener = listener

        if (sensorManager != null && accelerometer != null) {
            sensorManager.registerListener(this, accelerometer, SensorManager.SENSOR_DELAY_UI)
        }
    }

    fun unregister() {
        if (sensorManager != null && accelerometer != null) {
            sensorManager.unregisterListener(this)
        }
        shakeListener = null
    }
}
