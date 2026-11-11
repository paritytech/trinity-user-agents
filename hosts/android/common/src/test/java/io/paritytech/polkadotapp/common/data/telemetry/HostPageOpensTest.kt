package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HostPageOpensTest {
    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private val opens = HostPageOpens({ origin }, { events += it() })
    private val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L)

    @Test
    fun `holding an open reports it with its source and the id minted at the tap`() {
        opens.hold("loyalty.dot", tap)

        assertEquals(
            HostEvent(
                name = "host.pocket.opened",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "debug",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "open.source" to "pocket_card",
                    "host.session_id" to tap.sessionId,
                    "open.reused" to "false",
                ),
                extras = mapOf("host.app_build" to 123L),
            ),
            events.single(),
        )
        assertEquals(tap.sessionId, opens.take("loyalty.dot", nowEpochMs = 2_000L)?.sessionId)
    }

    @Test
    fun `an open for a page that is already open is reported as reused`() {
        opens.hold("loyalty.dot", HostPageOpen(HostOpenSource.DEEPLINK), reused = true)

        assertEquals("deeplink", events.single().tags["open.source"])
        assertEquals("true", events.single().tags["open.reused"])
    }

    @Test
    fun `each open mints its own lowercase uuid v4`() {
        val ids = List(2) { HostPageOpen(HostOpenSource.POCKET_CARD).sessionId }

        assertEquals(2, ids.toSet().size)
        ids.forEach { assertTrue(Regex("[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").matches(it)) }
    }

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
