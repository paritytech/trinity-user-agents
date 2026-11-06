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
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionRegistry
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HostBulletinFetchTelemetryTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> {
        every { nowMillis() } returnsMany listOf(LOOKUP_STARTED_AT, FETCH_STARTED_AT, FETCH_STARTED_AT, FETCHED_AT)
    }
    private val sessions = HostSessionRegistry()
    private val captured = mutableListOf<HostEvent>()
    private val telemetry = HostBulletinFetchTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() }, sessions)

    private val contentHashOverrides = mockk<ContentHashOverrides> {
        every { getContentHashOverride(APP_DOMAIN) } returns CONTENT_HASH
    }
    private val contentStorage = mockk<DotNsContentStorage>()
    private val carFetcher = mockk<CarFetcher>()

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.fromFile(any()) } returns mockk()
        sessions.open("humanity.dot", "session-42")
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    @Test
    fun `a downloaded bundle reports its size, the content hash read and the fetch under the product session`() =
        runTest {
            val car = temporaryFolder.newFile().apply { writeBytes(ByteArray(BUNDLE_BYTES.toInt())) }
            every { contentStorage.getContentDirectory(CONTENT_HASH) } returnsMany listOf(null, temporaryFolder.root)
            coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.success(car)
            coEvery { contentStorage.saveContentStreaming(CONTENT_HASH, any()) } just Runs

            resolver().resolveToLocalUri(APP_DOMAIN)

            assertEquals(
                HostEvent(
                    name = "host.bulletin.fetch",
                    tags = mapOf(
                        "host.platform" to "android",
                        "host.app_variant" to "nightly",
                        "host.app_flavor" to "gp",
                        "host.native_app_version" to "1.2.3",
                        "outcome" to "ok",
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
                ),
                captured.single(),
            )
        }

    @Test
    fun `a bundle already on disk is a cache hit with no download size`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns temporaryFolder.root

        resolver().resolveToLocalUri(APP_DOMAIN)

        val event = captured.single()
        assertEquals("cache_hit", event.tags["outcome"])
        assertFalse(event.extras.containsKey("bulletin.bytes"))
    }

    @Test
    fun `a download that fails is reported as an error`() = runTest {
        every { contentStorage.getContentDirectory(CONTENT_HASH) } returns null
        coEvery { carFetcher.fetchCarToFile(any(), any()) } returns Result.failure(IllegalStateException("gateway down"))

        resolver().resolveToLocalUri(APP_DOMAIN)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `the event is not built when capture drops it`() {
        HostBulletinFetchTelemetry(originProvider, timeProvider, HostEventCapture { }, sessions)
            .report(APP_DOMAIN, HostEventOutcome.OK, 30L, FETCH_STARTED_AT, BUNDLE_BYTES)

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

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
        const val CONTENT_HASH = "abcd"
        const val BUNDLE_BYTES = 2_048L
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
