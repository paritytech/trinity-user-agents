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
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuild
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
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
    private val contentHashOverrides: ContentHashOverrides = mockk()

    private val onDevice = mutableSetOf<String>()
    private val contentStorage: DotNsContentStorage = mockk {
        every { getContentDirectory(any()) } answers { firstArg<String>().takeIf { it in onDevice }?.let(::directoryOf) }
        coEvery { saveContentStreaming(any(), any()) } answers { onDevice += firstArg<String>() }
        coEvery { deleteContent(any()) } answers { onDevice -= firstArg<String>() }
    }

    private val builds = MutableStateFlow<Map<String, DotNsBuild>>(emptyMap())
    private val buildStorage: DotNsBuildStorage = mockk {
        // The matcher scope has a `get` of its own, so the mock's is named outright.
        every { this@mockk.get(any()) } answers { builds.value[firstArg()] }
        every { put(any(), any()) } answers { builds.value += firstArg<String>() to secondArg<DotNsBuild>() }
        every { all() } answers { builds.value }
        every { observeAll() } returns builds
    }

    private val domain = "peopl.testnet"
    private val alias = "people.testnet"
    private val releasedBuild = byteArrayOf(1, 2, 3)
    private val newBuild = byteArrayOf(4, 5, 6)

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.fromFile(any()) } answers { localUriOf(firstArg<File>()) }
        every { contentHashOverrides.getContentHashOverride(any()) } returns null
        coEvery { carFetcher.fetchCarToFile(any(), any()) } answers { Result.success(File.createTempFile("dotns", ".car")) }
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    // Serving is what a product needs offline and what must never wait on a node: the build on the device is the answer.
    @Test
    fun `a name on the device is served without chain`() = runTest {
        withOnDevice(served = releasedBuild)
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
        coVerify(exactly = 0) { contractApi.resolveContentHash(any()) }
    }

    @Test
    fun `a name not on the device is downloaded from the build chain points to and kept`() = runTest {
        withChainRecord(releasedBuild)
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
        assertEquals(DotNsBuild(served = hashOf(releasedBuild), update = null), builds.value[domain])
        assertEquals(setOf(domain), resolver.namesOnDevice())
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
    fun `a chain that cannot be reached fails a first load`() = runTest {
        withUnreachableChain()
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    // A node that drops out for a moment must not leave the product broken until the app restarts.
    @Test
    fun `a failed first load is retried on the next request`() = runTest {
        withUnreachableChain()
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        withChainRecord(releasedBuild)

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // A page fetches its scripts and styles one by one. Serving them from two builds would break it.
    @Test
    fun `requests keep the build served after a new one is released`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainRecord(newBuild)
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // The new build downloads beside the one in use, so the product keeps working and the switch costs nothing later.
    @Test
    fun `fetching an update downloads the new build and leaves the served one`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainRecord(newBuild)
        val resolver = resolver()

        assertEquals(Result.success(true), resolver.fetchUpdate(domain))

        assertTrue(hashOf(newBuild) in onDevice)
        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
        assertEquals(setOf(domain), resolver.observePendingUpdates().first())
    }

    @Test
    fun `fetching an update for a name on the latest build finds none`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainRecord(releasedBuild)
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdate(domain))

        coVerify(exactly = 0) { carFetcher.fetchCarToFile(any(), any()) }
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    // A publisher who rolls back must not have the rolled-back build offered as an update.
    @Test
    fun `a name published back to the served build drops its pending update`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        withChainRecord(releasedBuild)
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdate(domain))

        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
        assertTrue(hashOf(newBuild) !in onDevice)
    }

    @Test
    fun `a name with no build on the device has no update to fetch`() = runTest {
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdate(domain))
        coVerify(exactly = 0) { contractApi.resolveContentHash(any()) }
    }

    @Test
    fun `an update that fails to download is left for the next fetch`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainRecord(newBuild)
        withFailingDownload()
        val resolver = resolver()

        assertTrue(resolver.fetchUpdate(domain).isFailure)
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    // A background check that waits on a node forever would hold its worker until the system kills it.
    @Test
    fun `a chain that does not answer in time fails the update fetch`() = runTest {
        withOnDevice(served = releasedBuild)
        coEvery { contractApi.resolveContentHash(domain) } coAnswers { awaitCancellation() }
        val resolver = resolver()

        assertTrue(resolver.fetchUpdate(domain).isFailure)
    }

    // The bar on the host screen tracks the page being served; a download behind it would read as the page stalling.
    @Test
    fun `an update downloads without touching the progress of the name`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainRecord(newBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        resolver.fetchUpdate(domain)

        assertEquals(DotNsLoadProgress.Completed, resolver.getProgressByDomain(domain).first())
    }

    @Test
    fun `applying an update serves it and deletes the build it replaces`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        val resolver = resolver()

        resolver.applyUpdate(domain)

        assertServes(newBuild, resolver.resolveToLocalUri(domain))
        assertTrue(hashOf(releasedBuild) !in onDevice)
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    @Test
    fun `applying an update keeps a build another name still serves`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        withOnDevice(served = releasedBuild, name = alias)
        val resolver = resolver()

        resolver.applyUpdate(domain)

        assertServes(releasedBuild, resolver.resolveToLocalUri(alias))
    }

    @Test
    fun `applying with no update pending changes nothing`() = runTest {
        withOnDevice(served = releasedBuild)
        val resolver = resolver()

        resolver.applyUpdate(domain)

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    private fun withChainRecord(build: ByteArray?) {
        coEvery { contractApi.resolveContentHash(domain) } returns Result.success(build)
    }

    private fun withUnreachableChain() {
        coEvery { contractApi.resolveContentHash(domain) } returns Result.failure(IllegalStateException("no node"))
    }

    private fun withOnDevice(served: ByteArray, update: ByteArray? = null, name: String = domain) {
        onDevice += hashOf(served)
        update?.let { onDevice += hashOf(it) }
        builds.value += name to DotNsBuild(served = hashOf(served), update = update?.let(::hashOf))
    }

    private fun withFailingDownload() {
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IOException("gateway timeout"))
    }

    private fun assertServes(build: ByteArray, result: Result<Uri>) {
        assertEquals("served ${result.exceptionOrNull()}", directoryOf(hashOf(build)).absolutePath, result.getOrNull()?.path)
    }

    private fun hashOf(build: ByteArray): String = build.toHexString()

    private fun directoryOf(contentHash: String) = File("/dotns_content/$contentHash")

    private fun localUriOf(file: File): Uri = mockk { every { path } returns file.absolutePath }

    private fun TestScope.resolver() = RealDotNsResolver(
        contractApi = contractApi,
        carFetcher = carFetcher,
        contentStorage = contentStorage,
        buildStorage = buildStorage,
        contentHashOverrides = contentHashOverrides,
        dispatchers = testDispatchers(),
    )
}
