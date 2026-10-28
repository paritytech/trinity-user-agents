package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.IpfsHttpException
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuild
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameChange
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameVersion
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

private const val MANIFEST_KEY = "manifest"

class RealDotNsResolverTest {
    private val contractApi: DotNsContractApi = mockk()
    private val carFetcher: CarFetcher = mockk()
    private val contentHashOverrides: ContentHashOverrides = mockk()
    private val networkStateService: NetworkStateService = mockk {
        every { isNetworkAvailable } returns MutableStateFlow(true)
    }

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
        every { putAll(any()) } answers { builds.value += firstArg<Map<String, DotNsBuild>>() }
        every { all() } answers { builds.value }
        every { observeAll() } returns builds
    }

    private val domain = "peopl.testnet"
    private val alias = "people.testnet"
    private val worker = "worker.peopl.testnet"
    private val releasedBuild = byteArrayOf(1, 2, 3)
    private val newBuild = byteArrayOf(4, 5, 6)
    private val releasedWorker = byteArrayOf(7, 8, 9)
    private val newWorker = byteArrayOf(10, 11, 12)

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
        withChainContent(domain, releasedBuild)
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
        assertEquals(DotNsBuild(served = version(releasedBuild), update = null), builds.value[domain])
        assertEquals(setOf(domain), resolver.namesOnDevice())
    }

    // A host screen shows a spinner for as long as progress says the content is on its way. A domain
    // with nothing registered behind it never downloads and never unpacks, so unless the failure is
    // reported the spinner is the last thing the user ever sees.
    @Test
    fun `a domain with no content registered reports failure rather than staying silent`() = runTest {
        withChainContent(domain, null)
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

        withChainContent(domain, releasedBuild)

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // One dropped read on a slow network should cost a second, not a failed screen.
    @Test
    fun `a dropped chain read is tried again before the load gives up`() = runTest {
        coEvery { contractApi.resolveContentHash(domain) } returnsMany listOf(
            Result.failure(IOException("connection reset")),
            Result.success(releasedBuild),
        )
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    @Test
    fun `a gateway that has no such archive is not asked again`() = runTest {
        withChainContent(domain, releasedBuild)
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IpfsHttpException(404, "https://gateway/ipfs/cid"))
        val resolver = resolver()

        assertTrue(resolver.resolveToLocalUri(domain).isFailure)
        coVerify(exactly = 1) { carFetcher.fetchCarToFile(any(), any()) }
    }

    // The manifest is read on every start; kept on the device, a product opens without waiting on a node.
    @Test
    fun `a record read once is answered from the device without chain, also when it is empty`() = runTest {
        coEvery { contractApi.getMetadata(domain, MANIFEST_KEY) } returns Result.success(null)
        resolver().getMetadataEntry(domain, MANIFEST_KEY)

        val afterRestart = resolver()

        assertEquals(Result.success(null), afterRestart.getMetadataEntry(domain, MANIFEST_KEY))
        coVerify(exactly = 1) { contractApi.getMetadata(domain, MANIFEST_KEY) }
    }

    // A page fetches its scripts and styles one by one. Serving them from two builds would break it.
    @Test
    fun `requests keep the build served after a new one is released`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainContent(domain, newBuild)
        val resolver = resolver()

        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
    }

    // The new build downloads beside the one in use, so the product keeps working and the switch costs nothing later.
    @Test
    fun `fetching an update downloads the new build and leaves the served one`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainContent(domain, newBuild)
        val resolver = resolver()

        assertEquals(Result.success(true), resolver.fetchUpdates(listOf(domain)))

        assertTrue(hashOf(newBuild) in onDevice)
        assertServes(releasedBuild, resolver.resolveToLocalUri(domain))
        assertEquals(setOf(domain), resolver.observePendingUpdates().first())
    }

    @Test
    fun `fetching an update for a name on the latest build finds none`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainContent(domain, releasedBuild)
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdates(listOf(domain)))

        coVerify(exactly = 0) { carFetcher.fetchCarToFile(any(), any()) }
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    // A publisher who rolls back must not have the rolled-back build offered as an update.
    @Test
    fun `a name published back to the served build drops its pending update`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        withChainContent(domain, releasedBuild)
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdates(listOf(domain)))

        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
        assertTrue(hashOf(newBuild) !in onDevice)
    }

    @Test
    fun `a name with nothing on the device has no update to fetch`() = runTest {
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdates(listOf(domain)))
        coVerify(exactly = 0) { contractApi.resolveContentHash(any()) }
    }

    // A release whose page arrived and whose worker did not would run a new page against an old worker.
    @Test
    fun `an update that fails for one of the names is kept for none of them`() = runTest {
        withOnDevice(served = releasedBuild)
        withOnDevice(served = releasedWorker, name = worker)
        withChainContent(domain, newBuild)
        withChainContent(worker, newWorker)
        coEvery { carFetcher.fetchCarToFile(newWorker, any()) } returns Result.failure(IpfsHttpException(404, "https://gateway/ipfs/cid"))
        val resolver = resolver()

        assertTrue(resolver.fetchUpdates(listOf(domain, worker)).isFailure)
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    // A manifest change is part of the release: a product that adds a worker must not keep opening without it.
    @Test
    fun `a changed record is applied with the update`() = runTest {
        withOnDevice(served = releasedBuild, records = mapOf(MANIFEST_KEY to null))
        withChainContent(domain, releasedBuild)
        coEvery { contractApi.getMetadata(domain, MANIFEST_KEY) } returns Result.success("{\"name\":\"People\"}")
        val resolver = resolver()

        assertEquals(Result.success(true), resolver.fetchUpdates(listOf(domain)))
        assertEquals(Result.success(null), resolver.getMetadataEntry(domain, MANIFEST_KEY))

        resolver.applyUpdates(listOf(domain))

        assertEquals(Result.success("{\"name\":\"People\"}"), resolver.getMetadataEntry(domain, MANIFEST_KEY))
    }

    // A manifest product's base name carries only records; downloading content nothing serves would waste the data plan.
    @Test
    fun `content the device never loaded is left out of an update`() = runTest {
        withOnDevice(served = null, records = mapOf(MANIFEST_KEY to null))
        coEvery { contractApi.getMetadata(domain, MANIFEST_KEY) } returns Result.success(null)
        val resolver = resolver()

        assertEquals(Result.success(false), resolver.fetchUpdates(listOf(domain)))
        coVerify(exactly = 0) { contractApi.resolveContentHash(any()) }
    }

    @Test
    fun `an update that fails to download is left for the next fetch`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainContent(domain, newBuild)
        withFailingDownload()
        val resolver = resolver()

        assertTrue(resolver.fetchUpdates(listOf(domain)).isFailure)
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    // A background check that waits on a node forever would hold its worker until the system kills it.
    @Test
    fun `a chain that does not answer in time fails the update fetch`() = runTest {
        withOnDevice(served = releasedBuild)
        coEvery { contractApi.resolveContentHash(domain) } coAnswers { awaitCancellation() }
        val resolver = resolver()

        assertTrue(resolver.fetchUpdates(listOf(domain)).isFailure)
    }

    // The bar on the host screen tracks the page being served; a download behind it would read as the page stalling.
    @Test
    fun `an update downloads without touching the progress of the name`() = runTest {
        withOnDevice(served = releasedBuild)
        withChainContent(domain, newBuild)
        val resolver = resolver()
        resolver.resolveToLocalUri(domain)

        resolver.fetchUpdates(listOf(domain))

        assertEquals(DotNsLoadProgress.Completed, resolver.getProgressByDomain(domain).first())
    }

    @Test
    fun `applying an update serves it and deletes the build it replaces`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        val resolver = resolver()

        resolver.applyUpdates(listOf(domain))

        assertServes(newBuild, resolver.resolveToLocalUri(domain))
        assertTrue(hashOf(releasedBuild) !in onDevice)
        assertEquals(emptySet<String>(), resolver.observePendingUpdates().first())
    }

    @Test
    fun `applying an update keeps a build another name still serves`() = runTest {
        withOnDevice(served = releasedBuild, update = newBuild)
        withOnDevice(served = releasedBuild, name = alias)
        val resolver = resolver()

        resolver.applyUpdates(listOf(domain))

        assertServes(releasedBuild, resolver.resolveToLocalUri(alias))
    }

    private fun withChainContent(name: String, build: ByteArray?) {
        coEvery { contractApi.resolveContentHash(name) } returns Result.success(build)
    }

    private fun withUnreachableChain() {
        coEvery { contractApi.resolveContentHash(domain) } returns Result.failure(IllegalStateException("no node"))
    }

    private fun withOnDevice(
        served: ByteArray?,
        update: ByteArray? = null,
        name: String = domain,
        records: Map<String, String?> = emptyMap(),
    ) {
        served?.let { onDevice += hashOf(it) }
        update?.let { onDevice += hashOf(it) }
        builds.value += name to DotNsBuild(
            served = DotNsNameVersion(contentHash = served?.let(::hashOf), records = records),
            update = update?.let { DotNsNameChange(contentHash = hashOf(it), records = emptyMap()) },
        )
    }

    private fun withFailingDownload() {
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IOException("gateway timeout"))
    }

    private fun version(build: ByteArray) = DotNsNameVersion(contentHash = hashOf(build), records = emptyMap())

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
        networkStateService = networkStateService,
        dispatchers = testDispatchers(),
    )
}
