package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductIdTest {
    private val dot = DotNsTld.parse("dot")!!
    private val paseo = DotNsTld.parse("paseo")!!

    @Test
    fun `accepts a name under the active tld`() {
        assertEquals("coinflip.dot", ProductId.fromString("coinflip.dot", dot).getOrThrow().value)
        assertEquals("browse.paseo", ProductId.fromString("browse.paseo", paseo).getOrThrow().value)
    }

    @Test
    fun `accepts a subdomain name under the active tld`() {
        assertEquals("arena.coinflip.paseo", ProductId.fromString("arena.coinflip.paseo", paseo).getOrThrow().value)
    }

    @Test
    fun `rejects a name from another network's namespace`() {
        assertTrue(ProductId.fromString("browse.dot", paseo).isFailure)
        assertTrue(ProductId.fromString("browse.paseo", dot).isFailure)
    }

    @Test
    fun `rejects a name whose last label merely ends with the tld label`() {
        assertTrue(ProductId.fromString("coinflip.xpaseo", paseo).isFailure)
        assertTrue(ProductId.fromString("xpaseo", paseo).isFailure)
    }

    @Test
    fun `rejects a bare label without any tld`() {
        assertTrue(ProductId.fromString("coinflip", dot).isFailure)
    }

    @Test
    fun `is case-insensitive, since dotNS is`() {
        assertEquals("coinflip.dot", ProductId.fromString("Coinflip.DOT", dot).getOrThrow().value)
    }

    @Test
    fun `drops the executable label, so identity never depends on which surface was opened`() {
        listOf("app", "widget", "worker").forEach { kind ->
            assertEquals("coinflip.dot", ProductId.fromString("$kind.coinflip.dot", dot).getOrThrow().value)
        }
    }

    @Test
    fun `keeps a label that is the whole product name`() {
        // `app.dot` is a legitimate product; dropping its label would leave the bare TLD.
        assertEquals("app.dot", ProductId.fromString("app.dot", dot).getOrThrow().value)
    }

    @Test
    fun `rejects malformed names`() {
        listOf("example.com", "dot", "coin_flip.dot", "coinflip.dotcom", "", "  ", "coinflip.dot/x")
            .forEach { assertTrue("expected '$it' to be rejected", ProductId.fromString(it, dot).isFailure) }
    }
}
