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

    // Shaderlab does not clip the tilt, so a phone tipped past vertical keeps turning the material.
    @Test
    fun `a phone held upright tilts fully up, and past vertical tilts past 1`() {
        val upright = MaterialInputs().angles(pitched(90.0))
        val tipped = MaterialInputs().angles(pitched(100.0))
        val tilt = { angles: DeviceAngles -> MaterialInputs.frameInputs(0f, angles.beta, angles.gamma, angles.alpha, identity, still = false).tilt }

        assertAngles(DeviceAngles(0f, 90f, 0f), upright)
        assertArrayEquals(floatArrayOf(0f, 1f), tilt(upright), TOLERANCE)
        assertTrue(tilt(tipped)[1] > 1f)
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
