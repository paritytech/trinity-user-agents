package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HostPageOpensTest {
    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private val opens = HostPageOpens({ origin }, { events += it() })
    private val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, sessionId = "tap-session")
    private val link = HostPageOpen(HostOpenSource.DEEPLINK, epochMs = 1_500L, sessionId = "link-session")

    @Test
    fun `an open is not reported until something settles it`() {
        opens.hold("loyalty.dot", tap)

        assertEquals(emptyList<HostEvent>(), events)
    }

    @Test
    fun `a session taking an open reports it as a fresh open with the id minted at the tap`() {
        opens.hold("loyalty.dot", tap)

        assertEquals(tap, opens.take("loyalty.dot", nowEpochMs = 2_000L))

        assertEquals(
            HostEvent(
                name = "host.pocket.opened",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "debug",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "open.source" to "pocket_card",
                    "host.session_id" to "tap-session",
                    "open.reused" to "false",
                ),
                extras = mapOf("host.app_build" to 123L),
                isFailure = false,
            ),
            events.single(),
        )
    }

    @Test
    fun `an open for a page that is already open is reported as reused and never taken`() {
        opens.hold("loyalty.dot", link)

        opens.reuse("loyalty.dot")

        assertEquals("deeplink", events.single().tags["open.source"])
        assertEquals("true", events.single().tags["open.reused"])
        assertNull(opens.take("loyalty.dot", nowEpochMs = 2_000L))
        assertEquals(1, events.size)
    }

    @Test
    fun `an open given up before a page started is reported once and never taken`() {
        opens.hold("loyalty.dot", tap)

        opens.giveUp("loyalty.dot")

        assertEquals("false", events.single().tags["open.reused"])
        assertNull(opens.take("loyalty.dot", nowEpochMs = 2_000L))
        assertEquals(1, events.size)
    }

    @Test
    fun `an open is settled only for the product it was held for`() {
        opens.hold("loyalty.dot", tap)

        assertNull(opens.take("other.dot", nowEpochMs = 2_000L))
        opens.reuse("other.dot")
        opens.giveUp("other.dot")

        assertEquals(emptyList<HostEvent>(), events)
        assertEquals(tap, opens.take("loyalty.dot", nowEpochMs = 2_000L))
        assertNull(opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }

    @Test
    fun `a later ask replaces an earlier one no page took, which is reported as it goes`() {
        opens.hold("loyalty.dot", tap)
        opens.hold("loyalty.dot", link)

        assertEquals("tap-session", events.single().tags["host.session_id"])
        assertEquals("false", events.single().tags["open.reused"])
        assertEquals(link, opens.take("loyalty.dot", nowEpochMs = 2_000L))
    }

    @Test
    fun `an open no session took in time is reported but not handed to the session`() {
        opens.hold("loyalty.dot", tap)

        assertNull(opens.take("loyalty.dot", nowEpochMs = 1_000L + HostPageOpens.MAX_AGE_MS + 1))

        assertEquals("tap-session", events.single().tags["host.session_id"])
    }

    @Test
    fun `each open mints its own lowercase uuid v4 and reads the clock where it was asked for`() {
        val before = System.currentTimeMillis()
        val asks = List(2) { HostPageOpen.now(HostOpenSource.POCKET_CARD) }
        val after = System.currentTimeMillis()

        assertEquals(2, asks.map { it.sessionId }.toSet().size)
        asks.forEach {
            assertTrue(Regex("[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").matches(it.sessionId))
            assertTrue(it.epochMs in before..after)
        }
    }
}
