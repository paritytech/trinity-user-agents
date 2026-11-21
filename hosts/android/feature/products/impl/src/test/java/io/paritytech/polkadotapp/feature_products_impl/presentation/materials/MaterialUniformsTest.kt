package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ImageRef
import uniffi.truapi.MaterialShader
import uniffi.truapi.Parameter
import uniffi.truapi.Stage
import uniffi.truapi.Uniform
import uniffi.truapi.UniformKind
import uniffi.truapi.UniformRole
import kotlin.math.roundToInt

class MaterialUniformsTest {
    private val shader = MaterialShader(
        vertex = "",
        fragment = "",
        uniforms = listOf(
            uniform("_shaderlab_target_flip", UniformKind.FLOAT, UniformRole.TargetFlip),
            uniform("sl_time", UniformKind.FLOAT, UniformRole.Time),
            uniform("sl_tilt", UniformKind.VEC2, UniformRole.Tilt),
            uniform("sl_resolution", UniformKind.VEC2, UniformRole.Resolution),
            uniform("sl_orientation", UniformKind.VEC3, UniformRole.Orientation),
            uniform("sl_rotation", UniformKind.MAT3, UniformRole.Rotation),
            uniform("p_strength", UniformKind.FLOAT, UniformRole.Argument(0u)),
            uniform("p_tint", UniformKind.VEC2, UniformRole.Argument(1u)),
            uniform("_mode_input", UniformKind.INT, UniformRole.SamplingMode(0u)),
        ),
        vertexBlockSize = 0u,
        fragmentBlockSize = 0u,
        textures = emptyList(),
        readsTime = true,
        readsMotion = true,
    )
    private val args = listOf(
        Parameter.Float(1.5f.toRawBits().toUInt()),
        Parameter.Vec2(listOf(0.25f.toRawBits().toUInt(), (-2f).toRawBits().toUInt())),
        Parameter.SamplerFloat(ImageRef.Noise),
    )
    private val rotation = listOf(0f, 1f, 0f, -1f, 0f, 0f, 0f, 0f, 1f)
    private val motion = DeviceMotion(alpha = 30f, beta = 45f, gamma = -9f, rotation = rotation)

    @Test
    fun `each uniform takes the frame input its role names, and arguments their IEEE 754 bits`() {
        val values = materialUniforms(
            shader,
            args,
            frameInputs(2f, motion, stilled = false, width = 300, height = 100),
            targetFlip = -1f,
            unfilterableSlots = emptySet(),
        )

        assertEquals(
            listOf(
                floats("_shaderlab_target_flip", UniformKind.FLOAT, -1f),
                floats("sl_time", UniformKind.FLOAT, 2f),
                floats("sl_tilt", UniformKind.VEC2, -0.1f, 0.5f),
                floats("sl_resolution", UniformKind.VEC2, 300f, 100f),
                floats("sl_orientation", UniformKind.VEC3, 30f, 45f, -9f),
                MaterialUniformValue.Floats("sl_rotation", UniformKind.MAT3, rotation),
                floats("p_strength", UniformKind.FLOAT, 1.5f),
                floats("p_tint", UniformKind.VEC2, 0.25f, -2f),
                MaterialUniformValue.Integer("_mode_input", 0),
            ),
            values,
        )
    }

    @Test
    fun `a stilled frame has no time, tilt or orientation, and rotation is the identity`() {
        assertEquals(
            FrameInputs(0f, listOf(0f, 0f), 300, 100, listOf(0f, 0f, 0f), IDENTITY_ROTATION),
            frameInputs(2f, motion, stilled = true, width = 300, height = 100),
        )
    }

    @Test
    fun `a frame with no motion reading yet keeps its time`() {
        assertEquals(
            FrameInputs(2f, listOf(0f, 0f), 300, 100, listOf(0f, 0f, 0f), IDENTITY_ROTATION),
            frameInputs(2f, motion = null, stilled = false, width = 300, height = 100),
        )
    }

    @Test
    fun `reduced motion stills every material, a missing sensor only those that read motion`() {
        assertEquals(
            listOf(true, true, false, true, false),
            listOf(
                motionStilled(reducedMotion = true, readsMotion = false, hasMotionSensor = true),
                motionStilled(reducedMotion = true, readsMotion = true, hasMotionSensor = true),
                motionStilled(reducedMotion = false, readsMotion = false, hasMotionSensor = false),
                motionStilled(reducedMotion = false, readsMotion = true, hasMotionSensor = false),
                motionStilled(reducedMotion = false, readsMotion = true, hasMotionSensor = true),
            ),
        )
    }

    @Test
    fun `an input the GPU cannot filter reads sampling mode 1, every other slot 0`() {
        val modes = materialUniforms(shader, args, frameInputs(2f, motion, stilled = false, width = 1, height = 1), 1f, setOf(0))
            .filterIsInstance<MaterialUniformValue.Integer>()

        assertEquals(listOf(MaterialUniformValue.Integer("_mode_input", 1)), modes)
    }

    @Test
    fun `angles follow the device past vertical, as the orientation event gives them`() {
        val cos45 = 0.70710677f
        val flat = listOf(1f, 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f)
        val topRaised45 = listOf(1f, 0f, 0f, 0f, cos45, -cos45, 0f, cos45, cos45)
        val topRaised135 = listOf(1f, 0f, 0f, 0f, -cos45, -cos45, 0f, cos45, -cos45)
        val rightEdgeDown30 = listOf(0.8660254f, 0f, 0.5f, 0f, 1f, 0f, -0.5f, 0f, 0.8660254f)
        val quarterTurnLeft = listOf(0f, -1f, 0f, 1f, 0f, 0f, 0f, 0f, 1f)
        val turnedLeftPastVerticalRightEdgeDown = listOf(
            -0.3535534f, 0.70710677f, 0.6123724f,
            0.8660254f, 0f, 0.5f,
            0.3535534f, 0.70710677f, -0.6123724f,
        )

        assertEquals(
            listOf(listOf(0, 0, 0), listOf(0, 45, 0), listOf(0, 135, 0), listOf(0, 0, 30), listOf(90, 0, 0), listOf(90, 135, 30)),
            listOf(flat, topRaised45, topRaised135, rightEdgeDown30, quarterTurnLeft, turnedLeftPastVerticalRightEdgeDown).map { rows ->
                deviceMotion(rows).let { listOf(it.alpha, it.beta, it.gamma).map { angle -> angle.roundToInt() } }
            },
        )
    }

    @Test
    fun `rotation is handed over column by column`() {
        val rows = listOf(1f, 2f, 3f, 4f, 5f, 6f, 7f, 8f, 9f)

        assertEquals(listOf(1f, 4f, 7f, 2f, 5f, 8f, 3f, 6f, 9f), deviceMotion(rows).rotation)
    }

    private fun uniform(name: String, kind: UniformKind, role: UniformRole) =
        Uniform(name = name, kind = kind, stage = Stage.FRAGMENT, offset = 0u, role = role)

    private fun floats(name: String, kind: UniformKind, vararg values: Float) =
        MaterialUniformValue.Floats(name, kind, values.toList())
}
