package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductNavigationTest {
    private val dot = DotNsTld.parse("dot")!!
    private val paseo = DotNsTld.parse("paseo")!!

    @Test
    fun `accepts a name under the active tld`() {
        assertEquals("coinflip.dot", productIdFromHost("coinflip.dot", dot).getOrThrow().value)
        assertEquals("browse.paseo", productIdFromHost("browse.paseo", paseo).getOrThrow().value)
    }

    @Test
    fun `accepts a subdomain name under the active tld`() {
        assertEquals("arena.coinflip.paseo", productIdFromHost("arena.coinflip.paseo", paseo).getOrThrow().value)
    }

    @Test
    fun `rejects a name from another network's namespace`() {
        assertTrue(productIdFromHost("browse.dot", paseo).isFailure)
        assertTrue(productIdFromHost("browse.paseo", dot).isFailure)
    }

    @Test
    fun `rejects a name whose last label merely ends with the tld label`() {
        assertTrue(productIdFromHost("coinflip.xpaseo", paseo).isFailure)
        assertTrue(productIdFromHost("xpaseo", paseo).isFailure)
    }

    @Test
    fun `rejects a bare label without any tld`() {
        assertTrue(productIdFromHost("coinflip", dot).isFailure)
    }

    @Test
    fun `is case-insensitive, since dotNS is`() {
        assertEquals("coinflip.dot", productIdFromHost("Coinflip.DOT", dot).getOrThrow().value)
    }

    @Test
    fun `drops the executable label, so identity never depends on which surface was opened`() {
        listOf("app", "widget", "worker").forEach { kind ->
            assertEquals("coinflip.dot", productIdFromHost("$kind.coinflip.dot", dot).getOrThrow().value)
        }
    }

    @Test
    fun `keeps a label that is the whole product name`() {
        // `app.dot` is a legitimate product; dropping its label would leave the bare TLD.
        assertEquals("app.dot", productIdFromHost("app.dot", dot).getOrThrow().value)
    }

    @Test
    fun `rejects malformed names`() {
        listOf("example.com", "dot", "coin_flip.dot", "coinflip.dotcom", "", "  ", "coinflip.dot/x")
            .forEach { assertTrue("expected '$it' to be rejected", productIdFromHost(it, dot).isFailure) }
    }
}
