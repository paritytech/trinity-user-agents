package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HostPageOpensTest {
    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private var nowMs = 2_000L
    private val opens = HostPageOpens({ origin }, { events += it() }, { nowMs })
    private val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = 1_000L, sessionId = "tap-session")
    private val link = HostPageOpen(HostOpenSource.DEEPLINK, epochMs = 1_500L, monotonicMs = 1_500L, sessionId = "link-session")

    @Test
    fun `an open is not reported until something settles it`() {
        opens.hold("loyalty.dot", tap)

        assertEquals(emptyList<HostEvent>(), events)
    }

    @Test
    fun `a session taking an open reports it as a fresh open and adopts the id minted at the tap`() {
        opens.hold("loyalty.dot", tap)

        assertEquals(HostPageSession("loyalty.dot", "tap-session", tap, isRetry = false), opens.startSession("loyalty.dot"))

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
    fun `a session nothing asked for mints its own id and reports nothing`() {
        val session = opens.startSession("loyalty.dot")

        assertNull(session.open)
        assertTrue(session.id.isNotEmpty())
        assertNotEquals(session.id, opens.startSession("loyalty.dot").id)
        assertEquals(emptyList<HostEvent>(), events)
    }

    @Test
    fun `an open that shows a running page again is reported as reused under that page's session id`() {
        opens.hold("loyalty.dot", tap)
        val page = opens.startSession("loyalty.dot")
        events.clear()

        opens.hold("loyalty.dot", link)
        opens.reuse("loyalty.dot")

        val reused = events.single()
        assertEquals("deeplink", reused.tags["open.source"])
        assertEquals("true", reused.tags["open.reused"])
        assertEquals(page.id, reused.tags["host.session_id"])
    }

    @Test
    fun `a reused open is reported under the id of the session the page minted when nothing asked for it`() {
        val page = opens.startSession("loyalty.dot")

        opens.hold("loyalty.dot", tap)
        opens.reuse("loyalty.dot")

        assertEquals(page.id, events.single().tags["host.session_id"])
    }

    @Test
    fun `a reused open is reported under the latest session of its own product`() {
        opens.startSession("other.dot")
        val older = opens.startSession("loyalty.dot")
        val latest = opens.startSession("loyalty.dot")
        opens.sessionEnded(older)

        opens.hold("loyalty.dot", tap)
        opens.reuse("loyalty.dot")

        assertEquals(latest.id, events.single().tags["host.session_id"])
    }

    @Test
    fun `a reused open keeps its own id once the page's session has ended`() {
        opens.sessionEnded(opens.startSession("loyalty.dot"))

        opens.hold("loyalty.dot", link)
        opens.reuse("loyalty.dot")

        assertEquals("link-session", events.single().tags["host.session_id"])
    }

    @Test
    fun `a reused open is never taken by a later session`() {
        opens.hold("loyalty.dot", link)

        opens.reuse("loyalty.dot")

        assertNull(opens.startSession("loyalty.dot").open)
        assertEquals(1, events.size)
    }

    @Test
    fun `an open given up before a page started is reported once and never taken`() {
        opens.hold("loyalty.dot", tap)

        opens.giveUp("loyalty.dot")

        assertEquals("false", events.single().tags["open.reused"])
        assertEquals("tap-session", events.single().tags["host.session_id"])
        assertNull(opens.startSession("loyalty.dot").open)
        assertEquals(1, events.size)
    }

    @Test
    fun `an open is settled only for the product it was held for`() {
        opens.hold("loyalty.dot", tap)

        assertNull(opens.startSession("other.dot").open)
        opens.reuse("other.dot")
        opens.giveUp("other.dot")

        assertEquals(emptyList<HostEvent>(), events)
        assertEquals(tap, opens.startSession("loyalty.dot").open)
        assertNull(opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `a later ask replaces an earlier one no page took, which is reported as it goes`() {
        opens.hold("loyalty.dot", tap)
        opens.hold("loyalty.dot", link)

        assertEquals("tap-session", events.single().tags["host.session_id"])
        assertEquals("false", events.single().tags["open.reused"])
        assertEquals(link, opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `an open no session took in time still gives the session its id, but no time to load from`() {
        opens.hold("loyalty.dot", tap)
        nowMs = 1_000L + HostPageOpens.MAX_AGE_MS + 1

        val session = opens.startSession("loyalty.dot")

        assertEquals(HostPageSession("loyalty.dot", "tap-session", open = null, isRetry = false), session)
        assertEquals("tap-session", events.single().tags["host.session_id"])
    }

    @Test
    fun `a session that ended before its page was shown leaves its open for the next session to retry`() {
        opens.hold("loyalty.dot", tap)
        val first = opens.startSession("loyalty.dot")
        opens.sessionEnded(first)

        val retry = opens.startSession("loyalty.dot")

        assertEquals(HostPageSession("loyalty.dot", "tap-session", tap, isRetry = true), retry)
        // The open itself was reported once, by the session that took it.
        assertEquals(1, events.size)
        // Only once: a third session has nothing to retry while the second runs.
        assertNull(opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `a retry that also ends unshown leaves the open for the next one`() {
        opens.hold("loyalty.dot", tap)
        opens.sessionEnded(opens.startSession("loyalty.dot"))
        opens.sessionEnded(opens.startSession("loyalty.dot"))

        assertEquals(tap, opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `a session whose page was shown leaves nothing to retry`() {
        opens.hold("loyalty.dot", tap)
        val first = opens.startSession("loyalty.dot")
        opens.pageShown(first)
        opens.sessionEnded(first)

        val next = opens.startSession("loyalty.dot")

        assertNull(next.open)
        assertNotEquals("tap-session", next.id)
    }

    @Test
    fun `a new ask wins over an open left to retry`() {
        opens.hold("loyalty.dot", tap)
        opens.sessionEnded(opens.startSession("loyalty.dot"))

        opens.hold("loyalty.dot", link)

        assertEquals(HostPageSession("loyalty.dot", "link-session", link, isRetry = false), opens.startSession("loyalty.dot"))
        assertNull(opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `an open left to retry is dropped once it is too old to time from`() {
        opens.hold("loyalty.dot", tap)
        opens.sessionEnded(opens.startSession("loyalty.dot"))
        nowMs = tap.monotonicMs + HostPageOpens.MAX_AGE_MS + 1

        val next = opens.startSession("loyalty.dot")

        assertNull(next.open)
        assertNotEquals("tap-session", next.id)
    }

    @Test
    fun `an open left to retry belongs to its own product only`() {
        opens.hold("loyalty.dot", tap)
        opens.sessionEnded(opens.startSession("loyalty.dot"))

        assertNull(opens.startSession("other.dot").open)
        assertEquals(tap, opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `an open's age is read on the monotonic clock, so a wall clock set back does not make it new`() {
        // Asked an hour of wall time "in the future": the phone's clock was then set back.
        opens.hold("loyalty.dot", tap.copy(epochMs = 3_600_000L))
        nowMs = tap.monotonicMs + HostPageOpens.MAX_AGE_MS + 1

        assertNull(opens.startSession("loyalty.dot").open)
    }

    @Test
    fun `each open mints its own lowercase uuid v4 and reads both clocks where it was asked for`() {
        nowMs = 4_242L
        val before = System.currentTimeMillis()
        val asks = List(2) { opens.ask(HostOpenSource.POCKET_CARD) }
        val after = System.currentTimeMillis()

        assertEquals(2, asks.map { it.sessionId }.toSet().size)
        asks.forEach {
            assertTrue(Regex("[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").matches(it.sessionId))
            assertTrue(it.epochMs in before..after)
            assertEquals(4_242L, it.monotonicMs)
            assertEquals(HostOpenSource.POCKET_CARD, it.source)
        }
    }
}
