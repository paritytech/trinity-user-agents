package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class DotNsLoadProgressRegistryTest {
    private val registry = DotNsLoadProgressRegistry()

    @Test
    fun `unobserved domain starts idle`() {
        assertEquals(DotNsLoadProgress.Idle, registry.observe("a.dot").value)
    }

    @Test
    fun `transitions through resolving, downloading, unpacking, and completed`() {
        val domain = "a.dot"
        val flow = registry.observe(domain)

        registry.markResolving(domain)
        assertEquals(DotNsLoadProgress.Resolving, flow.value)

        registry.markDownloadProgress(domain, downloaded = 50, total = 100)
        assertEquals(DotNsLoadProgress.Downloading(fraction = 0.5f), flow.value)

        registry.markUnpacking(domain)
        assertEquals(DotNsLoadProgress.Unpacking, flow.value)

        registry.markCompleted(domain)
        assertEquals(DotNsLoadProgress.Completed, flow.value)
    }

    @Test
    fun `download fraction is coerced into 0 to 1`() {
        val domain = "a.dot"
        registry.markDownloadProgress(domain, downloaded = 150, total = 100)
        assertEquals(DotNsLoadProgress.Downloading(fraction = 1f), registry.observe(domain).value)
    }

    @Test
    fun `unknown total yields a null download fraction`() {
        val domain = "a.dot"
        registry.markDownloadProgress(domain, downloaded = 50, total = null)
        assertEquals(DotNsLoadProgress.Downloading(fraction = null), registry.observe(domain).value)
    }

    @Test
    fun `zero total yields a null download fraction`() {
        val domain = "a.dot"
        registry.markDownloadProgress(domain, downloaded = 0, total = 0)
        assertEquals(DotNsLoadProgress.Downloading(fraction = null), registry.observe(domain).value)
    }

    @Test
    fun `failure carries the cause`() {
        val domain = "a.dot"
        val cause = IllegalStateException("boom")
        registry.markFailed(domain, cause)

        val state = registry.observe(domain).value
        assertTrue(state is DotNsLoadProgress.Failed)
        assertSame(cause, (state as DotNsLoadProgress.Failed).cause)
    }

    @Test
    fun `domains are tracked independently`() {
        registry.markCompleted("a.dot")
        registry.markResolving("b.dot")

        assertEquals(DotNsLoadProgress.Completed, registry.observe("a.dot").value)
        assertEquals(DotNsLoadProgress.Resolving, registry.observe("b.dot").value)
    }
}
