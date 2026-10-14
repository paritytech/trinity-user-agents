package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DotWebHostTest {
    @Test
    fun `canonicalizes each mirror to its own root`() {
        assertEquals("coinflip.dot", "coinflip.dot.li".toCanonicalDotHost())
        assertEquals("coinflip.paseo", "coinflip.paseo.li".toCanonicalDotHost())
        assertEquals("coinflip.test", "coinflip.test.li".toCanonicalDotHost())
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
        assertFalse(isDotWebMirrorHost("coinflip.xpaseo.li"))
        assertEquals("coinflip.xpaseo.li", "coinflip.xpaseo.li".toCanonicalDotHost())
    }

    @Test
    fun `recognizes mirror hosts`() {
        assertTrue(isDotWebMirrorHost("coinflip.dot.li"))
        assertTrue(isDotWebMirrorHost("coinflip.paseo.li"))
        assertTrue(isDotWebMirrorHost("coinflip.test.li"))
        assertFalse(isDotWebMirrorHost("coinflip.dot"))
    }
}
