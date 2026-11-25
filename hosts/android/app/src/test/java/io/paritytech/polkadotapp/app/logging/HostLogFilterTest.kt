package io.paritytech.polkadotapp.app.logging

import org.junit.Assert.assertEquals
import org.junit.Test

class HostLogFilterTest {
    @Test
    fun `a log this host did not write is turned away`() {
        val filter = HostLogFilter()

        assertEquals(HostLogVerdict.Drop, filter.verdict(null, "a line from somewhere else"))
        assertEquals(HostLogVerdict.Drop, filter.verdict("BearerTokenAuthenticator", "refresh failed"))
        assertEquals(HostLogVerdict.Drop, filter.verdict("", "refresh failed"))
    }

    @Test
    fun `a host tag is sent`() {
        val filter = HostLogFilter()

        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.chain", "connection 1 closing"))
    }

    @Test
    fun `a looping tag sends a bounded number of lines and then counts what it dropped`() {
        var now = 0L
        val filter = HostLogFilter { now }
        val limit = HostLogFilter.LINES_PER_WINDOW

        val sent = (0 until limit + 500).count {
            filter.verdict("truapi.chain", "follow p:$it restarted") != HostLogVerdict.Drop
        }

        assertEquals(limit, sent)
        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.auth", "session restored"))

        now += HostLogFilter.WINDOW_MS

        assertEquals(HostLogVerdict.Send(500), filter.verdict("truapi.chain", "follow p:9999 restarted"))
        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.chain", "follow p:10000 restarted"))
    }

    @Test
    fun `a window that dropped nothing counts nothing`() {
        var now = 0L
        val filter = HostLogFilter { now }

        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.chain", "first"))
        now += HostLogFilter.WINDOW_MS
        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.chain", "second"))
    }

    @Test
    fun `each core marker has its own window`() {
        var now = 0L
        val filter = HostLogFilter { now }
        val limit = HostLogFilter.LINES_PER_WINDOW

        repeat(limit + 10) { filter.verdict("truapi.core", "truapi.tracing.warn: chain call failed $it") }

        assertEquals(
            HostLogVerdict.Send(0),
            filter.verdict("truapi.core", "truapi.chain.follow_budget_reached: c1 open 4s"),
        )

        now += HostLogFilter.WINDOW_MS

        assertEquals(
            HostLogVerdict.Send(10),
            filter.verdict("truapi.core", "truapi.tracing.warn: chain call failed again"),
        )
    }

    /**
     * A core event's body is its marker on its own, with no fields after it, so it counts against
     * the same window as the core line of that marker.
     */
    @Test
    fun `a core event counts against its own marker`() {
        val filter = HostLogFilter { 0 }
        val limit = HostLogFilter.LINES_PER_WINDOW

        repeat(limit) { filter.verdict("truapi.core", "truapi.funding.step_reached") }

        assertEquals(HostLogVerdict.Drop, filter.verdict("truapi.core", "truapi.funding.step_reached"))
        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.core", "truapi.funding.ended"))
    }
}
