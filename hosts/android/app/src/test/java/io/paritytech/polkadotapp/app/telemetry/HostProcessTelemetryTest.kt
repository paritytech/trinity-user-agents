package io.paritytech.polkadotapp.app.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class HostProcessTelemetryTest {
    private val metadata = HostProcessMetadata(
        appVersion = "1.2.3",
        appBuild = 42,
        appVariant = "nightly",
        platform = "android",
        osVersion = "15",
        truapiHostVersion = "0.24.0",
    )

    @Test
    fun `captures one process start event with stable launch id and numeric monotonic time`() {
        val expectedLaunchId = "608ad8ee-51f5-49ab-af80-e25fd72c434f"
        val captured = mutableListOf<HostProcessStartEvent>()
        val telemetry = HostProcessTelemetry(
            appLaunchId = expectedLaunchId,
            monotonicMillis = { 1_234L },
            metadata = { metadata },
            capture = captured::add,
        )

        telemetry.captureProcessStarted(enabled = true)

        assertEquals(
            HostProcessStartEvent(
                name = "host.process.started",
                appLaunchId = expectedLaunchId,
                monotonicMs = 1_234L,
                metadata = metadata,
            ),
            captured.single(),
        )
    }

    @Test
    fun `uses canonical cross-platform Sentry field names and native numeric values`() {
        val fields = HostProcessStartEvent(
            name = "host.process.started",
            appLaunchId = "608ad8ee-51f5-49ab-af80-e25fd72c434f",
            monotonicMs = 1_234L,
            metadata = metadata,
        ).sentryFields()

        assertEquals(
            mapOf(
                "app.launch_id" to "608ad8ee-51f5-49ab-af80-e25fd72c434f",
                "host.native_app_version" to "1.2.3",
                "host.app_variant" to "nightly",
                "host.platform" to "android",
                "host.os_version" to "15",
                "host.truapi_host_version" to "0.24.0",
            ),
            fields.tags,
        )
        assertEquals(
            mapOf(
                "host.app_build" to 42L,
                "event.monotonic_ms" to 1_234L,
            ),
            fields.extras,
        )
    }

    @Test
    fun `does nothing when telemetry is disabled`() {
        var metadataCalls = 0
        val captured = mutableListOf<HostProcessStartEvent>()
        val telemetry = HostProcessTelemetry(
            appLaunchId = "launch-id",
            monotonicMillis = { error("clock must not be read") },
            metadata = {
                metadataCalls += 1
                metadata
            },
            capture = captured::add,
        )

        telemetry.captureProcessStarted(enabled = false)

        assertEquals(0, metadataCalls)
        assertEquals(emptyList<HostProcessStartEvent>(), captured)
    }

    @Test
    fun `telemetry failures never escape app startup`() {
        val metadataFailure = HostProcessTelemetry(
            appLaunchId = "launch-id",
            monotonicMillis = { 1L },
            metadata = { error("native version unavailable") },
            capture = { error("must not capture") },
        )
        val captureFailure = HostProcessTelemetry(
            appLaunchId = "launch-id",
            monotonicMillis = { 1L },
            metadata = { metadata },
            capture = { error("Sentry unavailable") },
        )

        metadataFailure.captureProcessStarted(enabled = true)
        captureFailure.captureProcessStarted(enabled = true)
    }

    @Test
    fun `launch id belongs to the process telemetry instance`() {
        val captured = mutableListOf<HostProcessStartEvent>()
        val telemetry = HostProcessTelemetry(
            appLaunchId = "same-process-id",
            monotonicMillis = { 1L },
            metadata = { metadata },
            capture = captured::add,
        )

        telemetry.captureProcessStarted(enabled = true)
        telemetry.captureProcessStarted(enabled = true)

        assertSame(captured[0].appLaunchId, captured[1].appLaunchId)
    }
}
