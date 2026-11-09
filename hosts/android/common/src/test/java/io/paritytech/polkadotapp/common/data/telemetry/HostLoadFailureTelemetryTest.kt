package io.paritytech.polkadotapp.common.data.telemetry

import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.coroutines.cancellation.CancellationException

class HostLoadFailureTelemetryTest {
    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private val telemetry = HostLoadFailureTelemetry({ origin }, { events += it() })

    @Test
    fun `a failure is one error event tagged with its stage, product and session`() {
        telemetry.report(HostLoadStage.PAGE, IllegalStateException("boom"), "loyalty.dot", "session-1")

        assertEquals(
            HostEvent(
                name = "host.load.failed",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "debug",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "load.stage" to "page",
                    "error.type" to "IllegalStateException",
                    "product.id" to "loyalty.dot",
                    "host.session_id" to "session-1",
                ),
                extras = mapOf("host.app_build" to 123L, "error.message" to "boom"),
                isFailure = true,
            ),
            events.single(),
        )
    }

    @Test
    fun `a load cut short by closing its page is not a failure, one that timed out is`() {
        telemetry.report(HostLoadStage.WORKER, CancellationException("closed"), "loyalty.dot")
        assertTrue(events.isEmpty())

        val timeout = runBlocking {
            runCatching { withTimeout(1) { kotlinx.coroutines.awaitCancellation() } }.exceptionOrNull()
        }
        assertTrue(timeout is TimeoutCancellationException)
        telemetry.report(HostLoadStage.WORKER, timeout!!, "loyalty.dot")

        assertEquals("worker", events.single().tags["load.stage"])
    }
}
