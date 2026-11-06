package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Test

class HostCacheHitsTest {
    @Test
    fun `a memory hit is reported once per name, trigger and session`() {
        val hits = HostCacheHits()
        val open = HostLoadContext(HostLoadTrigger.OPEN, "page-session")

        val firsts = listOf(
            hits.isFirst("host.product.resolved", "peopl.dot", open),
            hits.isFirst("host.product.resolved", "peopl.dot", HostLoadContext(HostLoadTrigger.OPEN, "page-session")),
            hits.isFirst("host.product.resolved", "peopl.dot", HostLoadContext(HostLoadTrigger.PREWARM, null)),
            hits.isFirst("host.product.resolved", "peopl.dot", HostLoadContext(HostLoadTrigger.OPEN, "next-session")),
            hits.isFirst("host.product.resolved", "game.dot", open),
            hits.isFirst("host.bulletin.fetch", "peopl.dot", open),
        )

        assertEquals(listOf(true, false, true, true, true, true), firsts)
    }

    @Test
    fun `a forgotten session reports again while other sessions and prewarms stay claimed`() {
        val hits = HostCacheHits()
        val page = HostLoadContext(HostLoadTrigger.OPEN, "page-session")
        val other = HostLoadContext(HostLoadTrigger.OPEN, "other-session")
        val prewarm = HostLoadContext(HostLoadTrigger.PREWARM, null)
        listOf(page, other, prewarm).forEach { hits.isFirst("host.product.resolved", "peopl.dot", it) }

        hits.forget("page-session")

        assertEquals(
            listOf(true, false, false),
            listOf(page, other, prewarm).map { hits.isFirst("host.product.resolved", "peopl.dot", it) },
        )
    }
}
