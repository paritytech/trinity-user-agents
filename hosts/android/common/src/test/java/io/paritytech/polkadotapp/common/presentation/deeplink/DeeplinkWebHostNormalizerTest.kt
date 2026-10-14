package io.paritytech.polkadotapp.common.presentation.deeplink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DeeplinkWebHostNormalizerTest {
    // --- subdomain web mirror is a product: `<host>.dot.li` -> `<host>.dot` ---

    @Test
    fun `maps dot li product subdomain to canonical dot host`() {
        assertEquals("coinflip.dot", dotProductHost(scheme = "https", host = "coinflip.dot.li"))
    }

    @Test
    fun `maps paseo li product subdomain to its own paseo root`() {
        assertEquals("coinflip.paseo", dotProductHost(scheme = "https", host = "coinflip.paseo.li"))
    }

    @Test
    fun `maps test li product subdomain to its own test root`() {
        assertEquals("coinflip.test", dotProductHost(scheme = "https", host = "coinflip.test.li"))
    }

    @Test
    fun `maps an action-named subdomain to a product host too`() {
        assertEquals("pair.dot", dotProductHost(scheme = "https", host = "pair.dot.li"))
    }

    @Test
    fun `preserves multi-label product subdomain`() {
        assertEquals("a.b.dot", dotProductHost(scheme = "https", host = "a.b.dot.li"))
    }

    @Test
    fun `maps product subdomain on http scheme too`() {
        assertEquals("coinflip.dot", dotProductHost(scheme = "http", host = "coinflip.dot.li"))
    }

    @Test
    fun `product helper returns null for bare host without subdomain`() {
        assertNull(dotProductHost(scheme = "https", host = "dot.li"))
        assertNull(dotProductHost(scheme = "https", host = "paseo.li"))
    }

    @Test
    fun `product helper returns null for bare dot domain which is not a web mirror`() {
        assertNull(dotProductHost(scheme = "https", host = "coolapp.dot"))
    }

    @Test
    fun `product helper returns null for non deeplink domain`() {
        assertNull(dotProductHost(scheme = "https", host = "foo.com"))
    }

    @Test
    fun `product helper returns null for app scheme`() {
        assertNull(dotProductHost(scheme = "polkadotapp", host = "coinflip.dot"))
    }

    @Test
    fun `product helper returns null for missing scheme`() {
        assertNull(dotProductHost(scheme = null, host = "coinflip.dot.li"))
    }

    @Test
    fun `product helper returns null for missing host`() {
        assertNull(dotProductHost(scheme = "https", host = null))
    }

    // --- bare apex host carries the action in the path: `dot.li/<action>` -> `<action>` ---

    @Test
    fun `maps bare host first path segment to action`() {
        assertEquals("pair", bareHostDeeplinkAction(scheme = "https", host = "dot.li", pathSegments = listOf("pair")))
    }

    @Test
    fun `maps bare paseo host first path segment to action`() {
        assertEquals("pair", bareHostDeeplinkAction(scheme = "https", host = "paseo.li", pathSegments = listOf("pair")))
    }

    @Test
    fun `takes the first segment of a nested action path`() {
        assertEquals("pay", bareHostDeeplinkAction(scheme = "https", host = "dot.li", pathSegments = listOf("pay", "cheque")))
    }

    @Test
    fun `maps bare host on http scheme too`() {
        assertEquals("pair", bareHostDeeplinkAction(scheme = "http", host = "dot.li", pathSegments = listOf("pair")))
    }

    @Test
    fun `bare helper returns null without action path`() {
        assertNull(bareHostDeeplinkAction(scheme = "https", host = "dot.li", pathSegments = emptyList()))
        assertNull(bareHostDeeplinkAction(scheme = "https", host = "paseo.li", pathSegments = emptyList()))
    }

    @Test
    fun `bare helper ignores subdomain hosts`() {
        assertNull(bareHostDeeplinkAction(scheme = "https", host = "pair.dot.li", pathSegments = listOf("x")))
    }

    @Test
    fun `bare helper returns null for non deeplink domain`() {
        assertNull(bareHostDeeplinkAction(scheme = "https", host = "foo.com", pathSegments = listOf("x")))
    }

    @Test
    fun `bare helper returns null for missing scheme`() {
        assertNull(bareHostDeeplinkAction(scheme = null, host = "dot.li", pathSegments = listOf("pair")))
    }

    @Test
    fun `bare helper returns null for missing host`() {
        assertNull(bareHostDeeplinkAction(scheme = "https", host = null, pathSegments = listOf("pair")))
    }
}
