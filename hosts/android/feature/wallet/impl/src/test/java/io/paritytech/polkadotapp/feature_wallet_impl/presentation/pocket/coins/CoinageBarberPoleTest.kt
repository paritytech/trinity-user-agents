package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.barberPoleAxis
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.barberPoleOrigin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The Clearing rule's stripes, pinned by arithmetic rather than by eye.
 *
 * The animation runs a whole period in six hundred milliseconds, and screen captures a second apart alias it
 * badly enough to read as travelling either way. These three properties settle it.
 */
class CoinageBarberPoleTest {
    @Test
    fun `stripes lean right`() {
        val axis = barberPoleAxis(PERIOD)

        // The axis is perpendicular to the stripes, so an axis pointing right and up puts the stripes along
        // down-right, which is the lean a barber's pole has.
        assertTrue("axis should point right", axis.x > 0f)
        assertTrue("axis should point up, and screen y runs down", axis.y < 0f)
    }

    @Test
    fun `stripes travel rightwards`() {
        val phases = listOf(0f, 0.25f, 0.5f, 0.75f)
        val positions = phases.map { barberPoleOrigin(left = 0f, top = 0f, stripePeriod = PERIOD, phase = it).x }

        positions.zipWithNext { earlier, later ->
            assertTrue("the pattern should advance in +x, got $earlier then $later", later > earlier)
        }
    }

    @Test
    fun `a whole period of phase moves the origin exactly one period along the axis`() {
        val start = barberPoleOrigin(left = 0f, top = 0f, stripePeriod = PERIOD, phase = 0f)
        val wrapped = barberPoleOrigin(left = 0f, top = 0f, stripePeriod = PERIOD, phase = 1f)
        val travelled = (wrapped - start).getDistance()

        // Anything else and the stripes jump on every wrap.
        assertEquals("travel over one period", PERIOD, travelled, 1e-3f)
    }

    private companion object {
        const val PERIOD = 26f
    }
}
