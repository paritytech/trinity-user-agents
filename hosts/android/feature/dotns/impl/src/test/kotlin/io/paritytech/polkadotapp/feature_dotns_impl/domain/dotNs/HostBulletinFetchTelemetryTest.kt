package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HostBulletinFetchTelemetryTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> {
        every { nowMillis() } returnsMany
            listOf(ASKED_AT, LOOKUP_STARTED_AT, FETCH_STARTED_AT, FETCH_STARTED_AT, FETCHED_AT)
    }
    private val captured = mutableListOf<HostEvent>()
    private val telemetry =
        HostBulletinFetchTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() }, HostCacheHits())

    private val contentHashOverrides = mockk<ContentHashOverrides> {
        every { getContentHashOverride(APP_DOMAIN) } returns CONTENT_HASH
    }
    private val contentStorage = mockk<DotNsContentStorage>()
    private val carFetcher = mockk<CarFetcher>()

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.fromFile(any()) } returns mockk()
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    @Test
    fun `a downloaded bundle reports its size, the content hash read and the fetch under the open's session`() =
        runTest {
            val car = temporaryFolder.newFile().apply { writeBytes(ByteArray(BUNDLE_BYTES.toInt())) }
            every { contentStorage.getContentDirectory(CONTENT_HASH) } returnsMany listOf(null, temporaryFolder.root)
            coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.success(car)
            coEvery { contentStorage.saveContentStreaming(CONTENT_HASH, any()) } just Runs

            opening { resolver().resolveToLocalUri(APP_DOMAIN) }

            assertEquals(
                HostEvent(
                    name = "host.bulletin.fetch",
                    tags = mapOf(
                        "host.platform" to "android",
                        "host.app_variant" to "nightly",
                        "host.app_flavor" to "gp",
                        "host.native_app_version" to "1.2.3",
                        "outcome" to "ok",
                        "host.session_id" to "session-42",
                        "trigger" to "open",
                    ),
                    extras = mapOf(
                        "host.app_build" to 123L,
                        "event.monotonic_ms" to FETCHED_AT,
                        "duration_ms" to FETCHED_AT - FETCH_STARTED_AT,
                        "host.session_id" to "session-42",
                        "dotns.domain" to APP_DOMAIN,
                        "dotns.content_hash_ms" to FETCH_STARTED_AT - LOOKUP_STARTED_AT,
                        "bulletin.bytes" to BUNDLE_BYTES,
                    ),
                    durationMs = FETCHED_AT - FETCH_STARTED_AT,
                ),
                captured.single(),
            )
        }

    @Test
    fun `a bundle already on disk is a cache hit with no download size`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns temporaryFolder.root

        opening { resolver().resolveToLocalUri(APP_DOMAIN) }

        val event = captured.single()
        assertEquals("cache_hit", event.tags["outcome"])
        assertFalse(event.extras.containsKey("bulletin.bytes"))
    }

    // The resolver remembers a name for the whole process, so without this a warm open sends nothing
    // and reads as an open that never fetched. Once per session: the WebView asks on every request.
    @Test
    fun `a bundle this process already resolved is a cache hit once per session`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns temporaryFolder.root
        val resolver = resolver()

        opening { repeat(3) { resolver.resolveToLocalUri(APP_DOMAIN) } }

        assertEquals(listOf("cache_hit", "cache_hit"), captured.map { it.tags["outcome"] })
    }

    // Icons, chat names and face images read the same names; counted, they would read as opens.
    @Test
    fun `a lookup made outside any load reports nothing`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns temporaryFolder.root
        val resolver = resolver()

        repeat(2) { resolver.resolveToLocalUri(APP_DOMAIN) }

        assertEquals(emptyList<HostEvent>(), captured)
    }

    // A name whose content hash cannot be read never reaches the download, so it has to report itself.
    @Test
    fun `a name whose content hash cannot be read is reported as an error`() = runTest {
        every { contentHashOverrides.getContentHashOverride(APP_DOMAIN) } returns null
        val resolver = RealDotNsResolver(
            contractApi = mockk { coEvery { resolveContentHash(APP_DOMAIN) } returns Result.failure(IllegalStateException("rpc")) },
            carFetcher = carFetcher,
            contentStorage = contentStorage,
            contentHashOverrides = contentHashOverrides,
            fetchTelemetry = telemetry,
            dispatchers = testDispatchers(),
        )

        opening { resolver.resolveToLocalUri(APP_DOMAIN) }

        val event = captured.single()
        assertEquals("error", event.tags["outcome"])
        assertEquals("session-42", event.tags["host.session_id"])
    }

    // A prewarm runs before the page it is for, so the page's session must not be charged with it.
    @Test
    fun `a prewarm fetch is reported under its own trigger with no session`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns temporaryFolder.root

        withContext(HostLoadContext(HostLoadTrigger.PREWARM, sessionId = null)) {
            resolver().resolveToLocalUri(APP_DOMAIN)
        }

        val event = captured.single()
        assertEquals("prewarm", event.tags["trigger"])
        assertNull(event.tags["host.session_id"])
    }

    @Test
    fun `a download that fails is reported as an error`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns null
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IllegalStateException("gateway down"))

        opening { resolver().resolveToLocalUri(APP_DOMAIN) }

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `the event is not built when capture drops it`() {
        HostBulletinFetchTelemetry(originProvider, timeProvider, HostEventCapture { }, HostCacheHits())
            .report(APP_DOMAIN, OPEN, HostEventOutcome.OK, 30L, FETCH_STARTED_AT, BUNDLE_BYTES)

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    private suspend fun <T> opening(block: suspend () -> T): T = withContext(OPEN) { block() }

    private fun TestScope.resolver() = RealDotNsResolver(
        contractApi = mockk(),
        carFetcher = carFetcher,
        contentStorage = contentStorage,
        contentHashOverrides = contentHashOverrides,
        fetchTelemetry = telemetry,
        dispatchers = testDispatchers(),
    )

    private companion object {
        const val APP_DOMAIN = "app.humanity.dot"
        val OPEN = HostLoadContext(HostLoadTrigger.OPEN, "session-42")
        const val CONTENT_HASH = "abcd"
        const val BUNDLE_BYTES = 2_048L
        const val ASKED_AT = 990L
        const val LOOKUP_STARTED_AT = 1_000L
        const val FETCH_STARTED_AT = 1_030L
        const val FETCHED_AT = 1_900L
        val ORIGIN = HostEventOrigin(
            appVariant = "nightly",
            appFlavor = "gp",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "15",
            appBuild = 123L,
        )
    }
}
