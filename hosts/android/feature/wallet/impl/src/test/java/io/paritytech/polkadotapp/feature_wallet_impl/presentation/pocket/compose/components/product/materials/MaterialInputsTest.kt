package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.cos
import kotlin.math.sin

class MaterialInputsTest {
    private val identity = floatArrayOf(1f, 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f)

    // The device stood up by [degrees] about its own x axis, screen facing the user.
    private fun pitched(degrees: Double): FloatArray {
        val radians = Math.toRadians(degrees)
        val cosine = cos(radians).toFloat()
        val sine = sin(radians).toFloat()
        return floatArrayOf(1f, 0f, 0f, 0f, cosine, -sine, 0f, sine, cosine)
    }

    private fun assertAngles(expected: DeviceAngles, actual: DeviceAngles) {
        assertEquals(expected.alpha, actual.alpha, TOLERANCE)
        assertEquals(expected.beta, actual.beta, TOLERANCE)
        assertEquals(expected.gamma, actual.gamma, TOLERANCE)
    }

    @Test
    fun `a phone lying flat reads no angles`() {
        assertAngles(DeviceAngles(0f, 0f, 0f), MaterialInputs().angles(identity))
    }

    @Test
    fun `a phone held upright reads beta 90 and tilts fully up`() {
        val angles = MaterialInputs().angles(pitched(90.0))
        assertAngles(DeviceAngles(0f, 90f, 0f), angles)

        val frame = MaterialInputs.frameInputs(0f, angles.beta, angles.gamma, angles.alpha, identity, still = false)
        assertArrayEquals(floatArrayOf(0f, 1f), frame.tilt, TOLERANCE)
    }

    // Shaderlab does not clip the tilt, so a phone tipped past vertical keeps turning the material.
    @Test
    fun `a phone tipped past vertical tilts past 1`() {
        val angles = MaterialInputs().angles(pitched(100.0))
        val frame = MaterialInputs.frameInputs(0f, angles.beta, angles.gamma, angles.alpha, identity, still = false)

        assertEquals(100f, angles.beta, TOLERANCE)
        assertTrue(frame.tilt[1] > 1f)
    }

    // No warm-up, gain or smoothing: unlike the card tilt, every reading maps straight to its angles.
    @Test
    fun `every reading maps directly, the first included`() {
        val inputs = MaterialInputs()

        assertAngles(DeviceAngles(0f, 30f, 0f), inputs.angles(pitched(30.0)))
        assertAngles(DeviceAngles(0f, 60f, 0f), inputs.angles(pitched(60.0)))
    }

    @Test
    fun `a reading with no finite angle keeps the last one`() {
        val inputs = MaterialInputs()
        inputs.angles(pitched(30.0))

        assertAngles(DeviceAngles(0f, 30f, 0f), inputs.angles(FloatArray(9) { Float.NaN }))
    }

    @Test
    fun missingAlphaGivesZeroOrientationAndIdentityRotation() {
        val frame = MaterialInputs.frameInputs(2f, beta = 45f, gamma = -9f, alpha = null, rotation = null, still = false)

        assertArrayEquals(floatArrayOf(-0.1f, 0.5f), frame.tilt, TOLERANCE)
        assertArrayEquals(floatArrayOf(0f, 0f, 0f), frame.orientation, 0f)
        assertArrayEquals(identity, frame.rotation, 0f)
    }

    @Test
    fun `reduced motion holds the material still`() {
        val frame = MaterialInputs.frameInputs(3f, beta = 45f, gamma = 10f, alpha = 20f, rotation = pitched(45.0), still = true)

        assertStill(frame)
        assertEquals(0f, frame.time, 0f)
    }

    // A phone without a rotation sensor still animates in time; only motion stays at rest.
    @Test
    fun `no sensor reading keeps motion at rest and time running`() {
        val frame = MaterialInputs.frameInputs(3f, beta = null, gamma = null, alpha = null, rotation = null, still = false)

        assertStill(frame)
        assertEquals(3f, frame.time, 0f)
    }

    private fun assertStill(frame: FrameInputs) {
        assertArrayEquals(floatArrayOf(0f, 0f), frame.tilt, 0f)
        assertArrayEquals(floatArrayOf(0f, 0f, 0f), frame.orientation, 0f)
        assertArrayEquals(identity, frame.rotation, 0f)
    }

    private companion object {
        const val TOLERANCE = 0.01f
    }
}
