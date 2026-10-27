package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.parseProductShareHost

class DotWebHostTest {
    @Test
    fun `canonicalizes each mirror to its own root`() {
        assertEquals("coinflip.dot", "coinflip.dot.li".toCanonicalDotHost())
        assertEquals("coinflip.paseo", "coinflip.paseo.li".toCanonicalDotHost())
        assertEquals("coinflip.testnet", "coinflip.testnet.li".toCanonicalDotHost())
    }

    @Test
    fun `normalizes a mirror through the same rules as product identities`() {
        assertEquals("coinflip.paseo", "CoinFlip.PASEO.LI".toCanonicalDotHost())
    }

    @Test
    fun `keeps subdomains when canonicalizing`() {
        assertEquals("arena.coinflip.paseo", "arena.coinflip.paseo.li".toCanonicalDotHost())
    }

    @Test
    fun `leaves a non-mirror host unchanged`() {
        assertEquals("coinflip.paseo", "coinflip.paseo".toCanonicalDotHost())
        assertEquals("example.com", "example.com".toCanonicalDotHost())
    }

    @Test
    fun `does not treat a host merely ending with a mirror zone as a mirror`() {
        assertNull(parseProductShareHost("coinflip.xpaseo.li"))
        assertEquals("coinflip.xpaseo.li", "coinflip.xpaseo.li".toCanonicalDotHost())
    }

    @Test
    fun `rejects unsupported network and malformed product mirrors`() {
        listOf("coinflip.test.li", "coin_flip.dot.li", "localhost.dot.li", "coinflip..dot.li").forEach { host ->
            assertNull(host, parseProductShareHost(host))
            assertEquals(host, host.toCanonicalDotHost())
        }
    }

    @Test
    fun `recognizes mirror hosts`() {
        assertNotNull(parseProductShareHost("coinflip.dot.li"))
        assertNotNull(parseProductShareHost("coinflip.paseo.li"))
        assertNotNull(parseProductShareHost("coinflip.testnet.li"))
        assertNull(parseProductShareHost("coinflip.dot"))
    }
}
