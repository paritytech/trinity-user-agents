package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostPageOpensTest {
    private val opens = HostPageOpens()
    private val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L)

    @Test
    fun `an open is taken once, by the product it was held for`() {
        opens.hold("loyalty.dot", tap)

        assertNull(opens.take("other.dot", nowEpochMs = 2_000L))
        assertEquals(tap, opens.take("loyalty.dot", nowEpochMs = 2_000L))
        assertNull(opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }

    @Test
    fun `a later ask replaces an earlier one that no page took`() {
        val link = HostPageOpen(HostOpenSource.DEEPLINK, epochMs = 1_500L)
        opens.hold("loyalty.dot", tap)
        opens.hold("loyalty.dot", link)

        assertEquals(link, opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }

    @Test
    fun `an open no page took in time is dropped`() {
        opens.hold("loyalty.dot", tap)

        assertNull(opens.take("loyalty.dot", nowEpochMs = 1_000L + HostPageOpens.MAX_AGE_MS + 1))
    }

    @Test
    fun `an open discarded for a reused session is not taken by a later session of that product`() {
        opens.hold("loyalty.dot", tap)

        opens.discard("loyalty.dot")

        assertNull(opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }

    @Test
    fun `discarding one product leaves the open held for another`() {
        opens.hold("loyalty.dot", tap)

        opens.discard("other.dot")

        assertEquals(tap, opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }
}
