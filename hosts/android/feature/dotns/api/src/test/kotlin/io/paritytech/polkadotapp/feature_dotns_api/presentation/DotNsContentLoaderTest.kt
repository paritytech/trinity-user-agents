package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class DotNsContentLoaderTest {
    private val resolver: DotNsResolver = mockk()

    private val domain = "app.game.dot"
    private val releasedBuild: Uri = mockk()
    private val newBuild: Uri = mockk()
    private val nextBuild: Uri = mockk()

    // Opening a product has to show its new build without an app restart, even while other pages still run the old one.
    @Test
    fun `a page loads the latest build rather than the one in use elsewhere`() = runBlocking<Unit> {
        withBuildInUse(releasedBuild)
        withLatestBuild(newBuild)
        val loader = DotNsContentLoader(resolver)

        assertEquals(newBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    // A page loads its scripts after its document, and a link in a multi-page product loads another document. A new
    // build released in between would mix files the document knows nothing about, or drop the user's work unasked.
    @Test
    fun `a page keeps loading from the build its document came from`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveToLocalUri(domain)

        withLatestBuild(newBuild)

        assertEquals(releasedBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    @Test
    fun `a reload the user agreed to moves the page to the latest build`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveToLocalUri(domain)

        withLatestBuild(newBuild)
        loader.pickUpLatest()

        assertEquals(newBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    @Test
    fun `agreeing to one reload does not move the page to every later build`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveToLocalUri(domain)
        withLatestBuild(newBuild)
        loader.pickUpLatest()
        loader.resolveToLocalUri(domain)

        withLatestBuild(nextBuild)

        assertEquals(newBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    // The domain may already serve a newer build to other pages, so only the page's own build tells
    // whether this page is behind.
    @Test
    fun `a page is checked against the build its document came from`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        coEvery { resolver.isOutdated(domain, releasedBuild) } returns Result.success(true)
        val loader = DotNsContentLoader(resolver)
        loader.resolveToLocalUri(domain)

        assertEquals(true, loader.isDocumentOutdated(domain).getOrNull())
    }

    // The page on screen may be one the session never served, such as an ordinary web page.
    @Test
    fun `a domain with no document loaded is not outdated`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveToLocalUri(domain)

        assertEquals(false, loader.isDocumentOutdated("other.dot").getOrNull())
        coVerify(exactly = 0) { resolver.isOutdated(any(), any()) }
    }

    private fun withLatestBuild(build: Uri) {
        coEvery { resolver.resolveLatestToLocalUri(domain) } returns Result.success(build)
    }

    private fun withBuildInUse(build: Uri) {
        coEvery { resolver.resolveToLocalUri(domain) } returns Result.success(build)
    }
}
