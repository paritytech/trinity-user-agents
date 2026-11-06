package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostPocketTapsTest {
    private val taps = HostPocketTaps({ TAPPED_AT }, { })

    @Test
    fun `the page opened for the tapped card takes the tap, once`() {
        taps.tapped("humanity.dot")

        assertEquals(listOf(TAPPED_AT, null), listOf(taps.consume("humanity.dot"), taps.consume("humanity.dot")))
    }

    @Test
    fun `a page opened for another product does not take the tap`() {
        taps.tapped("humanity.dot")

        assertNull(taps.consume("game.dot"))
        assertEquals(TAPPED_AT, taps.consume("humanity.dot"))
    }

    // A held page is shown again without a new load, so its tap must not reach the next open.
    @Test
    fun `closing the card forgets its tap`() {
        taps.tapped("humanity.dot")

        taps.closed()

        assertNull(taps.consume("humanity.dot"))
    }

    @Test
    fun `no tap is kept while host telemetry is off`() {
        val off = HostPocketTaps({ error("no clock read while off") }, object : HostEventCapture {
            override val enabled = false

            override fun capture(event: () -> HostEvent) = Unit
        })

        off.tapped("humanity.dot")

        assertNull(off.consume("humanity.dot"))
    }

    private companion object {
        const val TAPPED_AT = 4_000L
    }
}
