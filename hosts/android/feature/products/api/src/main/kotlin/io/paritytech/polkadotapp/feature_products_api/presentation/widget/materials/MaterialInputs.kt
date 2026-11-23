package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import kotlin.math.PI
import kotlin.math.asin
import kotlin.math.atan2

internal data class DeviceAngles(val alpha: Float, val beta: Float, val gamma: Float)

internal class FrameInputs(
    val time: Float,
    val tilt: FloatArray,
    val orientation: FloatArray,
    val rotation: FloatArray,
)

// Device Orientation angles in degrees, derived from a row-major device-to-world matrix as browsers do.
internal class MaterialInputs {
    private var last = DeviceAngles(0f, 0f, 0f)

    fun angles(rotation: FloatArray): DeviceAngles {
        val read = deviceAngles(rotation)
        if (read.alpha.isFinite() && read.beta.isFinite() && read.gamma.isFinite()) last = read
        return last
    }

    companion object {
        private val IDENTITY = floatArrayOf(1f, 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f)
        private const val RIGHT_ANGLE = 90f

        fun frameInputs(
            seconds: Float,
            beta: Float?,
            gamma: Float?,
            alpha: Float?,
            rotation: FloatArray?,
            still: Boolean,
        ): FrameInputs {
            val time = if (still) 0f else seconds
            if (still || beta == null || gamma == null) {
                return FrameInputs(time, floatArrayOf(0f, 0f), floatArrayOf(0f, 0f, 0f), IDENTITY.copyOf())
            }
            return FrameInputs(
                time = time,
                tilt = floatArrayOf(gamma / RIGHT_ANGLE, beta / RIGHT_ANGLE),
                orientation = if (alpha == null) floatArrayOf(0f, 0f, 0f) else floatArrayOf(alpha, beta, gamma),
                rotation = rotation?.copyOf() ?: IDENTITY.copyOf(),
            )
        }

        private fun deviceAngles(matrix: FloatArray): DeviceAngles {
            val alpha: Double
            var beta: Double
            val gamma: Double
            when {
                matrix[8] > 0 -> {
                    alpha = atan2(-matrix[1].toDouble(), matrix[4].toDouble())
                    beta = asin(matrix[7].toDouble())
                    gamma = atan2(-matrix[6].toDouble(), matrix[8].toDouble())
                }
                matrix[8] < 0 -> {
                    alpha = atan2(matrix[1].toDouble(), -matrix[4].toDouble())
                    beta = -asin(matrix[7].toDouble())
                    beta += if (beta >= 0) -PI else PI
                    gamma = atan2(matrix[6].toDouble(), -matrix[8].toDouble())
                }
                matrix[6] > 0 -> {
                    alpha = atan2(-matrix[1].toDouble(), matrix[4].toDouble())
                    beta = asin(matrix[7].toDouble())
                    gamma = -PI / 2
                }
                matrix[6] < 0 -> {
                    alpha = atan2(matrix[1].toDouble(), -matrix[4].toDouble())
                    beta = -asin(matrix[7].toDouble())
                    beta += if (beta >= 0) -PI else PI
                    gamma = -PI / 2
                }
                else -> {
                    alpha = atan2(matrix[3].toDouble(), matrix[0].toDouble())
                    beta = if (matrix[7] > 0) PI / 2 else -PI / 2
                    gamma = 0.0
                }
            }
            val wrappedAlpha = if (alpha < 0) alpha + 2 * PI else alpha
            return DeviceAngles(wrappedAlpha.toDegrees(), beta.toDegrees(), gamma.toDegrees())
        }

        private fun Double.toDegrees(): Float = Math.toDegrees(this).toFloat()
    }
}
