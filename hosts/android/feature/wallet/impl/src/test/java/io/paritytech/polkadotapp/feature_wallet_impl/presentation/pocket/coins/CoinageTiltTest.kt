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
    fun `dipping the right edge makes a face-on coin mirror the right wall`() {
        val turn = CoinageTilt.studioTurn(lean = 0.0, sideways = CoinageTilt.scaled(0.4))
        val mirror = turned(0.0, 0.0, 1.0, turn)

        assertTrue("the coin should look toward the right wall", mirror[0] > 0.1)
        assertEquals("a sideways tilt is pure yaw, never a roll", 0.0, abs(turn.x) + abs(turn.z), TOLERANCE)
    }

    @Test
    fun `a sideways tilt turns the studio symmetrically`() {
        val left = CoinageTilt.studioTurn(lean = 0.0, sideways = CoinageTilt.scaled(-EIGHTH_TURN))
        val right = CoinageTilt.studioTurn(lean = 0.0, sideways = CoinageTilt.scaled(EIGHTH_TURN))

        assertEquals("equal and opposite", left.y, -right.y, TOLERANCE)
    }

    @Test
    fun `the studio turns as far as the phone`() {
        assertEquals(CoinageTilt.RANGE, CoinageTilt.TRAVEL, TOLERANCE)
    }

    @Test
    fun `the two turns compose as rotations rather than adding`() {
        val pitch = CoinageTilt.scaled(EIGHTH_TURN)
        val yaw = CoinageTilt.scaled(EIGHTH_TURN)
        val turn = CoinageTilt.leaned(pitch, yaw)

        // Added, the z component would be zero and the length would be exactly the hypotenuse of the two.
        assertTrue("the cross term is real", abs(turn.z) > TOLERANCE)
        assertTrue(
            "composed is not the sum",
            abs(sqrt(turn.x * turn.x + turn.y * turn.y + turn.z * turn.z) - hypot(pitch, yaw)) > TOLERANCE
        )
    }

    @Test
    fun `travel is clamped past the end of the range`() {
        assertEquals(CoinageTilt.TRAVEL, CoinageTilt.scaled(CoinageTilt.RANGE), TOLERANCE)
        assertEquals(CoinageTilt.TRAVEL, CoinageTilt.scaled(PI), TOLERANCE)
        assertEquals(-CoinageTilt.TRAVEL, CoinageTilt.scaled(-PI), TOLERANCE)
    }

    /**
     * A bearing between two components that both shrink is ill-conditioned exactly where the phone is held:
     * leaned back. Each angle is taken against everything left over instead, so a five degree roll reads as
     * five degrees however far back the phone is leaned.
     */
    @Test
    fun `a roll reads the same however far back the phone is leaned`() {
        val readings = listOf(0.0, 0.5, 1.0, 1.4).map { lean ->
            val upright = pose(roll = 0.0, lean = lean)

            pose(roll = FIVE_DEGREES, lean = lean).sideways - upright.sideways
        }

        for (reading in readings) {
            assertEquals("roll should read five degrees", FIVE_DEGREES, reading, 1e-6)
        }
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

    private fun hypot(a: Double, b: Double) = sqrt(a * a + b * b)

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
        const val EIGHTH_TURN = PI / 4
        const val FIVE_DEGREES = 5 * PI / 180
    }
}
