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
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class HostPocketFaceLoadedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private var now = REQUESTED_AT
    private val timeProvider = HostMonotonicTimeProvider { now }
    private val captured = mutableListOf<HostEvent>()
    private val telemetry = HostPocketFaceLoadedTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() })

    @Test
    fun `the first live face reports how long the card took to draw, from which worker and session`() = runTest {
        val load = telemetry.start(HUMANITY)
        load.worker(warm = false) { "worker-session" }
        now = SHOWN_AT

        load.track(flowOf("live 1", "live 2")).collect()

        assertEquals(
            HostEvent(
                name = "host.pocket.face_loaded",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "host.session_id" to "worker-session",
                    "product.id" to "humanity.dot",
                    "worker" to "cold",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to SHOWN_AT,
                    "duration_ms" to SHOWN_AT - REQUESTED_AT,
                    "host.session_id" to "worker-session",
                ),
                durationMs = SHOWN_AT - REQUESTED_AT,
            ),
            captured.single(),
        )
    }

    @Test
    fun `a face asked of a worker already running is warm`() = runTest {
        val load = telemetry.start(HUMANITY).apply { worker(warm = true) { null } }

        load.track(flowOf("live")).collect()

        assertEquals("warm", captured.single().tags["worker"])
    }

    @Test
    fun `the faces pass through unchanged`() = runTest {
        assertEquals(listOf("a", "b"), telemetry.start(HUMANITY).track(flowOf("a", "b")).toList())
    }

    @Test
    fun `a kept face is reported as a cache hit before the live outcome`() = runTest {
        val load = telemetry.start(HUMANITY)

        load.cachedFace()
        load.track(flowOf("live")).collect()

        assertEquals(listOf("cache_hit", "ok"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a face stream that fails before drawing anything is reported as an error`() = runTest {
        telemetry.start(HUMANITY).track(flow<String> { error("worker gone") }).catch { }.collect()

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a face stream that ends without drawing is reported as an error`() = runTest {
        telemetry.start(HUMANITY).track(emptyFlow<String>()).collect()

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a face no worker will draw is reported as no worker, and only that`() = runTest {
        val load = telemetry.start(HUMANITY)

        load.noWorker()
        load.track(emptyFlow<String>()).collect()

        assertEquals(listOf("no_worker"), captured.map { it.tags["outcome"] })
    }

    // Without it, a Pocket the user left before the face came is missing from the data.
    @Test
    fun `a Pocket closed before the live face arrives is reported as abandoned`() = runTest {
        val neverDrawn = flow<String> { awaitCancellation() }
        val onScreen = launch { telemetry.start(HUMANITY).track(neverDrawn).collect() }
        runCurrent()

        onScreen.cancelAndJoin()

        assertEquals(listOf("abandoned"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a face still missing after the timeout is reported as timed out, and a late face changes nothing`() =
        runTest {
            val late = flow {
                kotlinx.coroutines.delay(HostPocketFaceLoadedTelemetry.FACE_TIMEOUT * 2)
                emit("late")
            }
            val onScreen = launch { telemetry.start(HUMANITY).track(late).collect() }

            advanceTimeBy(HostPocketFaceLoadedTelemetry.FACE_TIMEOUT.inWholeMilliseconds + 1)
            assertEquals(listOf("timeout"), captured.map { it.tags["outcome"] })

            onScreen.join()
            assertEquals(listOf("timeout"), captured.map { it.tags["outcome"] })
        }

    @Test
    fun `the event is not built when capture drops it`() = runTest {
        HostPocketFaceLoadedTelemetry(originProvider, timeProvider, HostEventCapture { })
            .start(HUMANITY)
            .track(flowOf("live 1"))
            .collect()

        verify(exactly = 0) { originProvider.current() }
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
