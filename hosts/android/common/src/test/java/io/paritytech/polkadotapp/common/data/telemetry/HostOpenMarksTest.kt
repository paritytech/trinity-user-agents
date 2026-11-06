package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HostOpenMarksTest {
    private var now = TAPPED_AT
    private var minted = 0
    private val cacheHits = HostCacheHits()
    private val marks = HostOpenMarks({ now }, { TAPPED_AT_EPOCH }, { }, { "session-${++minted}" }, cacheHits)

    @Test
    fun `the page opened for the tapped card takes the tap and its session, once`() {
        marks.tapped("humanity.dot")

        val first = marks.consume("humanity.dot")

        assertEquals(
            listOf(TAPPED_AT, TAPPED_AT_EPOCH, HostOpenSource.POCKET_CARD, "session-1"),
            listOf(first?.atMillis, first?.atEpochMillis, first?.source, first?.sessionId),
        )
        assertNull(marks.consume("humanity.dot"))
    }

    @Test
    fun `the pending open's session is read for its own product only, without taking it`() {
        marks.tapped("humanity.dot")

        assertEquals(listOf("session-1", null), listOf(marks.pendingSession("humanity.dot"), marks.pendingSession("game.dot")))
        assertEquals("session-1", marks.consume("humanity.dot")?.sessionId)
        assertNull(marks.pendingSession("humanity.dot"))
    }

    @Test
    fun `each tap mints a session of its own`() {
        marks.tapped("humanity.dot")
        marks.tapped("humanity.dot")

        assertEquals("session-2", marks.consume("humanity.dot")?.sessionId)
    }

    @Test
    fun `a link is marked when it is handled and handed to the page it turns out to open`() {
        val link = marks.linkHandled()
        now = TAPPED_AT + 400

        marks.linked("humanity.dot", link)

        val taken = marks.consume("humanity.dot")
        assertEquals(listOf(TAPPED_AT, HostOpenSource.DEEPLINK, "session-1"), listOf(taken?.atMillis, taken?.source, taken?.sessionId))
    }

    // A link whose page never opened, such as one shown in a tab already open, must not time a later open.
    @Test
    fun `a mark older than the product accepts is dropped`() {
        marks.tapped("humanity.dot")
        now = TAPPED_AT + HostOpenMarks.PRODUCT_OPEN_TIME_MAX_AGE_MS + 1

        assertNull(marks.consume("humanity.dot"))
    }

    @Test
    fun `a page opened for another product does not take the tap`() {
        marks.tapped("humanity.dot")

        assertNull(marks.consume("game.dot"))
        assertEquals(TAPPED_AT, marks.consume("humanity.dot")?.atMillis)
    }

    // A held page is shown again without a new load, so its tap must not reach the next open.
    @Test
    fun `closing the card forgets its tap`() {
        marks.tapped("humanity.dot")

        marks.closed()

        assertNull(marks.consume("humanity.dot"))
    }

    @Test
    fun `an open no page took forgets what its session reported`() {
        val reportedUnder = { session: String -> HostLoadContext(HostLoadTrigger.OPEN, session) }
        marks.tapped("humanity.dot")
        cacheHits.isFirst("host.product.resolved", "humanity.dot", reportedUnder("session-1"))
        marks.tapped("game.dot")
        cacheHits.isFirst("host.product.resolved", "game.dot", reportedUnder("session-2"))
        marks.closed()
        marks.tapped("peopl.dot")
        cacheHits.isFirst("host.product.resolved", "peopl.dot", reportedUnder("session-3"))
        now = TAPPED_AT + HostOpenMarks.PRODUCT_OPEN_TIME_MAX_AGE_MS + 1
        marks.consume("peopl.dot")

        assertTrue(cacheHits.isFirst("host.product.resolved", "humanity.dot", reportedUnder("session-1")))
        assertTrue(cacheHits.isFirst("host.product.resolved", "game.dot", reportedUnder("session-2")))
        assertTrue(cacheHits.isFirst("host.product.resolved", "peopl.dot", reportedUnder("session-3")))
    }

    @Test
    fun `no tap is kept while host telemetry is off`() {
        val off = HostOpenMarks(
            { error("no clock read while off") },
            { error("no clock read while off") },
            object : HostEventCapture {
                override val enabled = false

                override fun capture(event: () -> HostEvent) = Unit
            },
            { error("no session minted while off") },
            cacheHits,
        )

        off.tapped("humanity.dot")

        assertNull(off.consume("humanity.dot"))
    }

    private companion object {
        const val TAPPED_AT = 4_000L
        const val TAPPED_AT_EPOCH = 1_793_880_000_000L
    }
}
