package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UrlsTest {
    @Test
    fun `isAbsoluteWebUrl should accept absolute http and https urls with a host`() {
        assertTrue(Urls.isAbsoluteWebUrl("https://cdn.example.com/cash/square.svg"))
        assertTrue(Urls.isAbsoluteWebUrl("http://cdn.example.com/cash/wide.png"))
        assertTrue(Urls.isAbsoluteWebUrl("HTTPS://cdn.example.com/x.svg"))
    }

    @Test
    fun `isAbsoluteWebUrl should reject relative and scheme-less urls`() {
        assertFalse(Urls.isAbsoluteWebUrl("square.svg"))
        assertFalse(Urls.isAbsoluteWebUrl("cdn.example.com/x.svg"))
        assertFalse(Urls.isAbsoluteWebUrl(""))
    }

    @Test
    fun `isAbsoluteWebUrl should reject non-web schemes`() {
        assertFalse(Urls.isAbsoluteWebUrl("ftp://cdn.example.com/square.svg"))
        assertFalse(Urls.isAbsoluteWebUrl("file:///etc/passwd"))
    }

    @Test
    fun `isAbsoluteWebUrl should reject malformed urls`() {
        assertFalse(Urls.isAbsoluteWebUrl("https://"))
        assertFalse(Urls.isAbsoluteWebUrl("not a url"))
        assertFalse(Urls.isAbsoluteWebUrl("https://exa mple.com/x.svg"))
    }
}
