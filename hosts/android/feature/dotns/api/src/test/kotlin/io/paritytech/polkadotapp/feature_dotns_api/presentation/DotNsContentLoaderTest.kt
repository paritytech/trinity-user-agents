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

    // A page loads its scripts after its document. If another page moved the domain to a new build in
    // between, those scripts would come from a build the document knows nothing about.
    @Test
    fun `a page keeps loading from the build its document came from`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)

        withBuildInUse(newBuild)

        assertEquals(releasedBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    // A link in a multi-page product loads a new document. Switching builds there would drop the user's work
    // without asking them.
    @Test
    fun `another document of the same domain stays on the page's build`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)

        withLatestBuild(newBuild)

        assertEquals(releasedBuild, loader.resolveLatestToLocalUri(domain).getOrNull())
        assertEquals(releasedBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    @Test
    fun `a reload the user agreed to moves the page to the latest build`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)

        withLatestBuild(newBuild)
        loader.pickUpLatestOnNextDocument()
        loader.resolveLatestToLocalUri(domain)

        assertEquals(newBuild, loader.resolveToLocalUri(domain).getOrNull())
    }

    @Test
    fun `only the next document after the user agreed moves to the latest build`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)
        withLatestBuild(newBuild)
        loader.pickUpLatestOnNextDocument()
        loader.resolveLatestToLocalUri(domain)

        withLatestBuild(nextBuild)

        assertEquals(newBuild, loader.resolveLatestToLocalUri(domain).getOrNull())
    }

    // The domain may already serve a newer build to other pages, so only the page's own build tells
    // whether this page is behind.
    @Test
    fun `a page is checked against the build its document came from`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        coEvery { resolver.isOutdated(domain, releasedBuild) } returns Result.success(true)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)

        assertEquals(true, loader.isDocumentOutdated(domain).getOrNull())
    }

    // The page on screen may be one the session never served, such as an ordinary web page.
    @Test
    fun `a domain with no document loaded is not outdated`() = runBlocking<Unit> {
        withLatestBuild(releasedBuild)
        val loader = DotNsContentLoader(resolver)
        loader.resolveLatestToLocalUri(domain)

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
