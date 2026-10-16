package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.io.IOException

class RealDotNsResolverTest {
    private val contractApi: DotNsContractApi = mockk()
    private val carFetcher: CarFetcher = mockk()
    private val contentStorage: DotNsContentStorage = mockk()
    private val contentHashOverrides: ContentHashOverrides = mockk()

    private val domain = "peopl.testnet"
    private val releasedBuild = byteArrayOf(1, 2, 3)
    private val newBuild = byteArrayOf(4, 5, 6)
    private val nextBuild = byteArrayOf(7, 8, 9)

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.fromFile(any()) } answers { localUriOf(firstArg<File>()) }
        every { contentHashOverrides.getContentHashOverride(domain) } returns null
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    // A host screen shows a spinner for as long as progress says the content is on its way. A domain
    // with nothing registered behind it never downloads and never unpacks, so unless the failure is
    // reported the spinner is the last thing the user ever sees.
    @Test
    fun `a domain with no content registered reports failure rather than staying silent`() = runTest {
        withChainRecord(null)
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    @Test
    fun `a chain that cannot be reached reports failure too`() = runTest {
        withUnreachableChain()
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    // A node that drops out for a moment must not leave the product broken until the app restarts.
    @Test
    fun `a failed resolution is retried on the next request`() = runTest {
        withUnreachableChain()
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // A page fetches its scripts and styles one by one. Serving them from two builds would break it.
    @Test
    fun `requests keep the build in use after a new one is released`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild, newBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(newBuild)

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // Without this, a player stays on the build they first opened until the app process dies.
    @Test
    fun `a document load switches to a build released after the first one`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild, newBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(newBuild)

        assertServes(newBuild, resolver.resolveLatestToLocalUri(domain))
        assertServes(newBuild, resolver.resolveToLocalUri(domain))
    }

    // Offline, the build on the device is still better than an error page.
    @Test
    fun `a document load keeps the build in use when the chain cannot be reached`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withUnreachableChain()

        assertServes(releasedBuild, resolver.resolveLatestToLocalUri(domain))
        assertEquals(DotNsLoadProgress.Completed, resolver.getProgressByDomain(domain).first())
    }

    @Test
    fun `a document load keeps the build in use when the new one fails to download`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(newBuild)
        withFailingDownload()

        assertServes(releasedBuild, resolver.resolveLatestToLocalUri(domain))
        assertEquals(DotNsLoadProgress.Completed, resolver.getProgressByDomain(domain).first())
    }

    // Reloading would land on the build in use again, and the user would lose their work for nothing on every visit.
    @Test
    fun `a build that failed to download is not offered`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(newBuild)
        withFailingDownload()
        resolver.resolveLatestToLocalUri(domain)

        assertEquals(false, resolver.isOutdated(domain, localUriOf(releasedBuild)).getOrNull())
    }

    @Test
    fun `a build released after one that failed to download is offered`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)
        withChainRecord(newBuild)
        withFailingDownload()
        resolver.resolveLatestToLocalUri(domain)

        withChainRecord(nextBuild)

        assertEquals(true, resolver.isOutdated(domain, localUriOf(releasedBuild)).getOrNull())
    }

    // The check runs while the user is in the product, so it must not start a download behind their back.
    @Test
    fun `a page is outdated once its domain points to another build, and nothing is downloaded`() = runTest {
        withChainRecord(newBuild)
        withDownloaded(releasedBuild)

        val result = resolver().isOutdated(domain, localUriOf(releasedBuild))

        assertEquals(true, result.getOrNull())
        verifyNothingDownloaded()
    }

    @Test
    fun `a page on the build its domain points to is not outdated`() = runTest {
        withChainRecord(releasedBuild)
        withDownloaded(releasedBuild)

        val result = resolver().isOutdated(domain, localUriOf(releasedBuild))

        assertEquals(false, result.getOrNull())
    }

    private fun withChainRecord(build: ByteArray?) {
        coEvery { contractApi.resolveContentHash(domain) } returns Result.success(build)
    }

    private fun withUnreachableChain() {
        coEvery { contractApi.resolveContentHash(domain) } returns Result.failure(IllegalStateException("no node"))
    }

    private fun withDownloaded(vararg builds: ByteArray) {
        val downloaded = builds.associateBy({ hashOf(it) }, ::directoryOf)
        every { contentStorage.getContentDirectory(any()) } answers { downloaded[firstArg()] }
    }

    private fun withFailingDownload() {
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IOException("gateway timeout"))
    }

    private fun verifyNothingDownloaded() {
        coVerify(exactly = 0) { carFetcher.fetchCarToFile(any(), any()) }
    }

    private fun assertServes(build: ByteArray, result: Result<Uri>) {
        assertEquals("served ${result.exceptionOrNull()}", directoryOf(build).absolutePath, result.getOrNull()?.path)
    }

    private fun hashOf(build: ByteArray): String = build.toHexString()

    private fun directoryOf(build: ByteArray) = File("/dotns_content/${hashOf(build)}")

    private fun localUriOf(build: ByteArray): Uri = localUriOf(directoryOf(build))

    private fun localUriOf(file: File): Uri = mockk { every { path } returns file.absolutePath }

    private fun TestScope.resolver() = RealDotNsResolver(
        contractApi = contractApi,
        carFetcher = carFetcher,
        contentStorage = contentStorage,
        contentHashOverrides = contentHashOverrides,
        dispatchers = testDispatchers(),
    )
}
