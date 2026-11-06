package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostPocketTapsTest {
    private var now = TAPPED_AT
    private val taps = HostPocketTaps({ now }, { TAPPED_AT_EPOCH }, { })

    @Test
    fun `the page opened for the tapped card takes the tap, once`() {
        taps.tapped("humanity.dot")

        val first = taps.consume("humanity.dot")

        assertEquals(TAPPED_AT to TAPPED_AT_EPOCH, first?.atMillis to first?.atEpochMillis)
        assertNull(taps.consume("humanity.dot"))
    }

    @Test
    fun `a link is marked when it is handled and handed to the page it turns out to open`() {
        val link = taps.linkHandled()
        now = TAPPED_AT + 400

        taps.linked("humanity.dot", link)

        val taken = taps.consume("humanity.dot")
        assertEquals(TAPPED_AT to HostOpenSource.DEEPLINK, taken?.atMillis to taken?.source)
    }

    // A link whose page never opened, such as one shown in a tab already open, must not time a later open.
    @Test
    fun `a mark older than the product accepts is dropped`() {
        taps.tapped("humanity.dot")
        now = TAPPED_AT + HostPocketTaps.PRODUCT_OPEN_TIME_MAX_AGE_MS + 1

        assertNull(taps.consume("humanity.dot"))
    }

    @Test
    fun `a page opened for another product does not take the tap`() {
        taps.tapped("humanity.dot")

        assertNull(taps.consume("game.dot"))
        assertEquals(TAPPED_AT, taps.consume("humanity.dot")?.atMillis)
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
        val off = HostPocketTaps({ error("no clock read while off") }, { error("no clock read while off") }, object : HostEventCapture {
            override val enabled = false

            override fun capture(event: () -> HostEvent) = Unit
        })

        off.tapped("humanity.dot")

        assertNull(off.consume("humanity.dot"))
    }

    private companion object {
        const val TAPPED_AT = 4_000L
        const val TAPPED_AT_EPOCH = 1_793_880_000_000L
    }
}
