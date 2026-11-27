package io.paritytech.polkadotapp.app.logging

import org.junit.Assert.assertEquals
import org.junit.Test

class HostLogFilterTest {
    @Test
    fun `only a tag this host writes is sent`() {
        val cases = listOf(
            null to false,
            "" to false,
            "BearerTokenAuthenticator" to false,
            "truapi.chain" to true,
            HostLogFilter.CORE_TAG to true,
        )

        cases.forEach { (tag, sent) ->
            val expected = if (sent) HostLogVerdict.Send(0) else HostLogVerdict.Drop

            assertEquals("tag $tag", expected, HostLogFilter { 0 }.verdict(tag, "connection 1 closing"))
        }
    }

    /** What a line counts against: its marker on `truapi.core`, and its tag where it names none. */
    @Test
    fun `two lines share a window only when they share a key`() {
        val long = "x".repeat(HostLogFilter.MARKER_LIMIT)
        val cases = listOf(
            Triple(core("truapi.tracing.warn: chain call failed"), core("truapi.tracing.warn: failed again"), true),
            Triple(core("truapi.funding.step_reached"), core("truapi.funding.step_reached: step 3 of 15"), true),
            Triple(core(long), core("$long and then some"), true),
            Triple(core(": fields with no marker before them"), core(long), true),
            Triple(tagged("truapi.chain"), tagged("truapi.chain"), true),
            Triple(core("truapi.tracing.warn: failed"), core("truapi.chain.follow_budget_reached: c1"), false),
            Triple(core("truapi.funding.step_reached"), core("truapi.funding.ended"), false),
            Triple(core(long), core("truapi.funding.ended"), false),
            Triple(tagged("truapi.chain"), tagged("truapi.auth"), false),
        )

        cases.forEach { (first, second, shared) ->
            val filter = HostLogFilter { 0 }
            repeat(HostLogFilter.LINES_PER_WINDOW) { filter.verdict(first.first, first.second) }
            val expected = if (shared) HostLogVerdict.Drop else HostLogVerdict.Send(0)

            assertEquals("$first then $second", expected, filter.verdict(second.first, second.second))
        }
    }

    @Test
    fun `a looping key sends a bounded number of lines then counts what it dropped`() {
        var now = 0L
        val filter = HostLogFilter { now }
        val limit = HostLogFilter.LINES_PER_WINDOW

        val sent = (0 until limit + 500).count {
            filter.verdict("truapi.chain", "follow p:$it restarted") != HostLogVerdict.Drop
        }

        assertEquals(limit, sent)

        now += HostLogFilter.WINDOW_MS

        assertEquals(HostLogVerdict.Send(500), filter.verdict("truapi.chain", "follow p:9999 restarted"))

        now += HostLogFilter.WINDOW_MS

        assertEquals(HostLogVerdict.Send(0), filter.verdict("truapi.chain", "follow p:10000 restarted"))
    }

    private fun core(body: String) = HostLogFilter.CORE_TAG to body

    private fun tagged(tag: String) = tag to "connection 1 closing"
}
