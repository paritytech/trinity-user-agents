package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import io.mockk.mockk
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/** The parts of the studio's turn that are arithmetic rather than sensor plumbing. */
class CoinageTiltTest {
    @Test
    fun `screen tilted to face upward makes a face-on coin mirror the ceiling`() {
        val neutral = pose(roll = 0.0, lean = 0.0)
        val turn = neutral.turnTo(pose(roll = 0.0, lean = 0.4))
        val mirror = turned(0.0, 0.0, 1.0, turn)

        assertTrue("the coin should look up into the room", mirror[1] > 0.1)
        assertEquals("a lean is pure pitch", 0.0, abs(turn.y) + abs(turn.z), TOLERANCE)
    }

    @Test
    fun `dipping the right edge of a phone lying flat makes a face-on coin mirror the right wall`() {
        val turn = flat(roll = 0.0).turnTo(flat(roll = 0.4))
        val mirror = turned(0.0, 0.0, 1.0, turn)

        assertTrue("the coin should look toward the right wall", mirror[0] > 0.1)
    }

    @Test
    fun `a sideways tilt of a leaned-back phone turns the studio symmetrically`() {
        val neutral = pose(roll = 0.0, lean = 0.9)
        val left = neutral.turnTo(pose(roll = -0.3, lean = 0.9))
        val right = neutral.turnTo(pose(roll = 0.3, lean = 0.9))

        assertEquals("no shared pitch", 0.0, left.x + right.x, 1e-9)
        assertEquals("equal and opposite", left.y, -right.y, 1e-9)
        assertEquals("equal and opposite", left.z, -right.z, 1e-9)
    }

    @Test
    fun `the turn is clamped to the range`() {
        val turn = pose(roll = 0.0, lean = 0.0).turnTo(pose(roll = 0.0, lean = 1.4))

        assertEquals(CoinageTilt.TRAVEL, sqrt(turn.x * turn.x + turn.y * turn.y + turn.z * turn.z), 1e-9)
    }

    @Test
    fun `travel is clamped past the end of the range`() {
        assertEquals(CoinageTilt.TRAVEL, CoinageTilt.scaled(CoinageTilt.RANGE), TOLERANCE)
        assertEquals(CoinageTilt.TRAVEL, CoinageTilt.scaled(PI), TOLERANCE)
        assertEquals(-CoinageTilt.TRAVEL, CoinageTilt.scaled(-PI), TOLERANCE)
    }

    /**
     * The renderer is woken only when the light has somewhere to go, and hand tremor is not somewhere to go.
     *
     * This cannot be caught on an emulator: its gravity sensor is synthetic and returns the same reading to
     * the last digit, so the loop parks there however tight the threshold is. On a phone the old threshold
     * sat under the sensor's own noise floor, and every reading redrew every coin.
     */
    @Test
    fun `tremor does not wake the renderer, a real tilt does`() {
        assertEquals("tremor woke the renderer", 0, wakeCount(List(TREMOR_READINGS) { TREMOR * if (it % 2 == 0) 1 else -1 }))
        assertEquals("a deliberate tilt did not wake it", 1, wakeCount(listOf(DELIBERATE)))
    }

    /** How many of [rolls], fed one reading at a time from square, asked for a frame. */
    private fun wakeCount(rolls: List<Double>): Int {
        val tilt = CoinageTilt(mockk(relaxed = true))
        var woken = 0

        tilt.onMove = { woken++ }
        // The first reading becomes neutral outright, so the light has nowhere to go from it.
        tilt.absorb(pose(roll = 0.0, lean = 0.0), FRAME)

        rolls.forEach { tilt.absorb(pose(roll = it, lean = 0.0), FRAME) }

        return woken
    }

    /** Gravity in the phone's frame for a device rolled and leaned by the given angles. */
    private fun pose(roll: Double, lean: Double): CoinageTilt.Pose {
        // Down, in the device frame: upright portrait is -y, leaning back tips it toward -z, rolling right
        // tips it toward +x.
        val x = kotlin.math.sin(roll)
        val flat = kotlin.math.cos(roll)

        return CoinageTilt.Pose.of(x, -flat * kotlin.math.cos(lean), -flat * kotlin.math.sin(lean))
    }

    /** Gravity for a phone lying flat, face up, with its right edge dipped by [roll]. */
    private fun flat(roll: Double) = CoinageTilt.Pose.of(sin(roll), 0.0, -cos(roll))

    /** Rodrigues' rotation, as the shader's `tiltDir` applies the turn to a mirror direction. */
    private fun turned(x: Double, y: Double, z: Double, turn: CoinageTilt.Turn): DoubleArray {
        val angle = sqrt(turn.x * turn.x + turn.y * turn.y + turn.z * turn.z)

        if (angle < 1e-12) return doubleArrayOf(x, y, z)

        val ax = turn.x / angle
        val ay = turn.y / angle
        val az = turn.z / angle
        val c = cos(angle)
        val s = sin(angle)
        val dot = ax * x + ay * y + az * z

        return doubleArrayOf(
            x * c + (ay * z - az * y) * s + ax * dot * (1 - c),
            y * c + (az * x - ax * z) * s + ay * dot * (1 - c),
            z * c + (ax * y - ay * x) * s + az * dot * (1 - c)
        )
    }

    private companion object {
        /** Well above a gravity sensor's noise floor, well below anything anyone means to do. */
        const val TREMOR = 0.3 * PI / 180
        const val DELIBERATE = 5.0 * PI / 180
        const val TREMOR_READINGS = 40
        const val FRAME = 1.0 / 60

        const val TOLERANCE = 1e-9
    }
}
