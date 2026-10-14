package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NetworkAccessPermissionHandlerTest {
    @Test
    fun `generateDomainCandidates should return wildcard chain for deep subdomain`() {
        val result = NetworkAccessPermissionHandler.generateDomainCandidates("deep.api.example.com")

        assertEquals(
            listOf("deep.api.example.com", "*.api.example.com", "*.example.com", "*"),
            result
        )
    }

    @Test
    fun `generateDomainCandidates should return exact and single wildcard for simple subdomain`() {
        val result = NetworkAccessPermissionHandler.generateDomainCandidates("api.example.com")

        assertEquals(
            listOf("api.example.com", "*.example.com", "*"),
            result
        )
    }

    @Test
    fun `generateDomainCandidates should return exact and universal wildcard for second-level domain`() {
        val result = NetworkAccessPermissionHandler.generateDomainCandidates("example.com")

        assertEquals(listOf("example.com", "*"), result)
    }

    @Test
    fun `generateDomainCandidates should return exact and universal wildcard for bare word`() {
        val result = NetworkAccessPermissionHandler.generateDomainCandidates("localhost")

        assertEquals(listOf("localhost", "*"), result)
    }

    @Test
    fun `generateDomainCandidates should handle many levels`() {
        val result = NetworkAccessPermissionHandler.generateDomainCandidates("a.b.c.d.e")

        assertEquals(
            listOf("a.b.c.d.e", "*.b.c.d.e", "*.c.d.e", "*.d.e", "*"),
            result
        )
    }

    @Test
    fun `extractDomain should return host from valid url`() {
        val result = NetworkAccessPermissionHandler.extractDomain("https://api.example.com/path")

        assertEquals("api.example.com", result)
    }

    @Test
    fun `extractDomain should return null for invalid url`() {
        val result = NetworkAccessPermissionHandler.extractDomain("not a url %%")

        assertNull(result)
    }

    @Test
    fun `extractDomain should return host without port`() {
        val result = NetworkAccessPermissionHandler.extractDomain("https://example.com:8080/path")

        assertEquals("example.com", result)
    }
}
