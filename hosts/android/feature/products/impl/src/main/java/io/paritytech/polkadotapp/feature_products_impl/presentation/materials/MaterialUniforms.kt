package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import uniffi.truapi.MaterialShader
import uniffi.truapi.Parameter
import uniffi.truapi.UniformKind
import uniffi.truapi.UniformRole
import kotlin.math.asin
import kotlin.math.atan2

data class DeviceMotion(
    val alpha: Float,
    val beta: Float,
    val gamma: Float,
    val rotation: List<Float>,
)

data class FrameInputs(
    val seconds: Float,
    val tilt: List<Float>,
    val width: Int,
    val height: Int,
    val orientation: List<Float>,
    val rotation: List<Float>,
)

sealed interface MaterialUniformValue {
    val name: String

    data class Floats(override val name: String, val kind: UniformKind, val values: List<Float>) : MaterialUniformValue

    data class Integer(override val name: String, val value: Int) : MaterialUniformValue
}

val IDENTITY_ROTATION = listOf(1f, 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f)

private val NO_TILT = listOf(0f, 0f)
private val NO_ORIENTATION = listOf(0f, 0f, 0f)

fun motionStilled(reducedMotion: Boolean, readsMotion: Boolean, hasMotionSensor: Boolean): Boolean =
    reducedMotion || (readsMotion && !hasMotionSensor)

fun frameInputs(seconds: Float, motion: DeviceMotion?, stilled: Boolean, width: Int, height: Int): FrameInputs = when {
    stilled -> FrameInputs(0f, NO_TILT, width, height, NO_ORIENTATION, IDENTITY_ROTATION)
    motion == null -> FrameInputs(seconds, NO_TILT, width, height, NO_ORIENTATION, IDENTITY_ROTATION)
    else -> FrameInputs(
        seconds = seconds,
        tilt = listOf(motion.gamma / RIGHT_ANGLE_DEGREES, motion.beta / RIGHT_ANGLE_DEGREES),
        width = width,
        height = height,
        orientation = listOf(motion.alpha, motion.beta, motion.gamma),
        rotation = motion.rotation,
    )
}

// The rows are Rz(alpha) Rx(beta) Ry(gamma). Gamma stays within 90 degrees, so the sign of
// cos(beta) cos(gamma), the last entry, says on which side of vertical beta lies.
fun deviceMotion(rows: List<Float>): DeviceMotion {
    val upright = rows[8] >= 0f
    val sign = if (upright) 1f else -1f
    val pitch = degrees(asin(rows[7].coerceIn(-1f, 1f)))
    return DeviceMotion(
        alpha = degrees(atan2(-sign * rows[1], sign * rows[4])).wrapFrom(0f),
        beta = (if (upright) pitch else HALF_TURN_DEGREES - pitch).wrapFrom(-HALF_TURN_DEGREES),
        gamma = degrees(atan2(-sign * rows[6], sign * rows[8])),
        rotation = List(MATRIX_SIZE) { index -> rows[(index % MATRIX_SIDE) * MATRIX_SIDE + index / MATRIX_SIDE] },
    )
}

private fun degrees(radians: Float): Float = Math.toDegrees(radians.toDouble()).toFloat()

private fun Float.wrapFrom(start: Float): Float = (this - start).mod(FULL_TURN_DEGREES) + start

// targetFlip is 1 into the window and -1 into a layer target, whose rows are read top first like an uploaded image.
fun materialUniforms(
    shader: MaterialShader,
    args: List<Parameter>,
    inputs: FrameInputs,
    targetFlip: Float,
    unfilterableSlots: Set<Int>,
): List<MaterialUniformValue> = shader.uniforms.mapNotNull { uniform ->
    val role = uniform.role
    if (role is UniformRole.SamplingMode) {
        MaterialUniformValue.Integer(uniform.name, if (role.v1.toInt() in unfilterableSlots) 1 else 0)
    } else {
        role.floats(args, inputs, targetFlip)?.let { floats -> MaterialUniformValue.Floats(uniform.name, uniform.kind, floats) }
    }
}

private fun UniformRole.floats(args: List<Parameter>, inputs: FrameInputs, targetFlip: Float): List<Float>? = when (this) {
    UniformRole.Time -> listOf(inputs.seconds)
    UniformRole.Tilt -> inputs.tilt
    UniformRole.Resolution -> listOf(inputs.width.toFloat(), inputs.height.toFloat())
    UniformRole.Orientation -> inputs.orientation
    UniformRole.Rotation -> inputs.rotation
    UniformRole.TargetFlip -> listOf(targetFlip)
    is UniformRole.Argument -> args.getOrNull(v1.toInt())?.floats()
    is UniformRole.SamplingMode -> null
}

private fun Parameter.floats(): List<Float>? = when (this) {
    is Parameter.Float -> listOf(Float.fromBits(v1.toInt()))
    is Parameter.Vec2 -> v1.map { Float.fromBits(it.toInt()) }
    is Parameter.Vec3 -> v1.map { Float.fromBits(it.toInt()) }
    is Parameter.Vec4 -> v1.map { Float.fromBits(it.toInt()) }
    is Parameter.SamplerFloat, is Parameter.SamplerVec2, is Parameter.SamplerVec3, is Parameter.SamplerVec4 -> null
}

const val MATRIX_SIZE = 9
private const val MATRIX_SIDE = 3
private const val RIGHT_ANGLE_DEGREES = 90f
private const val HALF_TURN_DEGREES = 180f
private const val FULL_TURN_DEGREES = 360f
