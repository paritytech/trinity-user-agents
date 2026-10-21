package io.paritytech.polkadotapp.app.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test
import java.util.UUID
import kotlin.time.Duration.Companion.milliseconds

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
    fun `captures one process start event with stable launch id and numeric process start time`() {
        val expectedLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f")
        val captured = mutableListOf<HostProcessStartEvent>()
        val telemetry = HostProcessTelemetry(
            appLaunchId = expectedLaunchId,
            processStartElapsedRealtime = { 1_234.milliseconds },
            metadata = { metadata },
            capture = captured::add,
        )

        telemetry.captureProcessStarted()

        assertEquals(
            HostProcessStartEvent(
                name = "host.process.started",
                appLaunchId = expectedLaunchId,
                processStartElapsedRealtime = 1_234.milliseconds,
                metadata = metadata,
            ),
            captured.single(),
        )
    }

    @Test
    fun `uses canonical cross-platform Sentry field names and native numeric values`() {
        val fields = HostProcessStartEvent(
            name = "host.process.started",
            appLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f"),
            processStartElapsedRealtime = 1_234.milliseconds,
            metadata = metadata,
        ).sentryFields()

        assertEquals(
            mapOf(
                "host.native_app_version" to "1.2.3",
                "host.app_variant" to "nightly",
                "host.platform" to "android",
                "host.os_version" to "15",
                "truapi.host_version" to "0.24.0",
            ),
            fields.tags,
        )
        assertEquals(
            mapOf(
                "app.launch_id" to "608ad8ee-51f5-49ab-af80-e25fd72c434f",
                "host.app_build" to 42L,
                "event.monotonic_ms" to 1_234L,
            ),
            fields.extras,
        )
    }

    @Test
    fun `does not invoke capture when telemetry is disabled`() {
        var captureCalls = 0

        val result = captureHostProcessStartedIfEnabled(enabled = false) {
            captureCalls += 1
            Result.success(Unit)
        }

        assertEquals(0, captureCalls)
        assertEquals(Result.success(Unit), result)
    }

    @Test
    fun `telemetry failures are returned for initializer logging`() {
        val metadataError = IllegalStateException("native version unavailable")
        val metadataFailure = HostProcessTelemetry(
            appLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f"),
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { throw metadataError },
            capture = { error("must not capture") },
        )
        val captureError = IllegalStateException("Sentry unavailable")
        val captureFailure = HostProcessTelemetry(
            appLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f"),
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { metadata },
            capture = { throw captureError },
        )

        val metadataResult = metadataFailure.captureProcessStarted()
        val captureResult = captureFailure.captureProcessStarted()

        assertSame(metadataError, metadataResult.exceptionOrNull())
        assertSame(captureError, captureResult.exceptionOrNull())
    }

    @Test
    fun `launch id belongs to the process telemetry instance`() {
        val captured = mutableListOf<HostProcessStartEvent>()
        val expectedLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f")
        val telemetry = HostProcessTelemetry(
            appLaunchId = expectedLaunchId,
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { metadata },
            capture = captured::add,
        )

        telemetry.captureProcessStarted()
        telemetry.captureProcessStarted()

        assertSame(captured[0].appLaunchId, captured[1].appLaunchId)
    }

    @Test
    fun `reports distribution flavor together with nightly build type`() {
        assertEquals("gpNightly", appVariant(flavor = "gp", buildType = "nightly"))
        assertEquals("vanillaNightly", appVariant(flavor = "vanilla", buildType = "nightly"))
    }
}
