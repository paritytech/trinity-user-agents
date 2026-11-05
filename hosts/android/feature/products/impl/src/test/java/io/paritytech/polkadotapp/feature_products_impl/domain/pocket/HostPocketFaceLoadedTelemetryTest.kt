package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class HostPocketFaceLoadedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> { every { nowMillis() } returns SHOWN_AT }
    private val captured = mutableListOf<HostEvent>()
    private val telemetry = HostPocketFaceLoadedTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() })

    @Test
    fun `the first live face reports how long the card took to draw since the Pocket asked for it`() = runTest {
        telemetry.reportFirstLiveFace(HUMANITY, REQUESTED_AT, flowOf("live 1", "live 2")).collect()

        assertEquals(
            HostEvent(
                name = "host.pocket.face_loaded",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "product.id" to "humanity.dot",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to SHOWN_AT,
                    "duration_ms" to SHOWN_AT - REQUESTED_AT,
                ),
            ),
            captured.single(),
        )
    }

    @Test
    fun `a kept face is reported as a cache hit`() {
        telemetry.reportCachedFace(HUMANITY, REQUESTED_AT)

        assertEquals(listOf("cache_hit"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a face stream that fails before drawing anything is reported as an error`() = runTest {
        val failing = flow<String> { error("worker gone") }

        telemetry.reportFirstLiveFace(HUMANITY, REQUESTED_AT, failing).catch { }.collect()

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a Pocket closed before the live face arrives reports nothing`() = runTest {
        val neverDrawn = flow<String> { awaitCancellation() }
        val onScreen = launch { telemetry.reportFirstLiveFace(HUMANITY, REQUESTED_AT, neverDrawn).collect() }
        runCurrent()

        onScreen.cancelAndJoin()

        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `the event is not built when capture drops it`() = runTest {
        HostPocketFaceLoadedTelemetry(originProvider, timeProvider, HostEventCapture { })
            .reportFirstLiveFace(HUMANITY, REQUESTED_AT, flowOf("live 1"))
            .collect()

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    private companion object {
        const val REQUESTED_AT = 30_000L
        const val SHOWN_AT = 30_850L
        val HUMANITY = ProductId.parse("humanity.dot").getOrThrow()
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
