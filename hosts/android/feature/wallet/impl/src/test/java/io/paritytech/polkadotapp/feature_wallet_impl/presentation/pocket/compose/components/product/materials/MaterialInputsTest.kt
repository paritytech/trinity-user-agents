package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
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

    @Test
    fun `reduced motion holds the material still`() {
        val frame = MaterialInputs.frameInputs(3f, beta = 45f, gamma = 10f, alpha = 20f, rotation = pitched(45.0), still = true)

        assertStill(frame)
        assertEquals(0f, frame.time, 0f)
    }

    private fun assertStill(frame: FrameInputs) {
        assertArrayEquals(floatArrayOf(0f, 0f), frame.tilt, 0f)
        assertArrayEquals(floatArrayOf(0f, 0f, 0f), frame.orientation, 0f)
        assertArrayEquals(identity, frame.rotation, 0f)
    }
}
