package io.paritytech.polkadotapp.app.telemetry

import io.paritytech.polkadotapp.test_shared.any
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.ArgumentCaptor
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
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
    fun `captures one event with stable launch id and numeric process start time when process starts`() {
        val expectedLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f")
        val capture = mock(HostProcessStartCapture::class.java)
        val telemetry = HostProcessTelemetry(
            appLaunchId = expectedLaunchId,
            processStartElapsedRealtime = { 1_234.milliseconds },
            metadata = { metadata },
            capture = capture,
        )

        val result = telemetry.captureProcessStarted()

        assertSuccess(result)
        verify(capture).capture(
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
    fun `uses canonical names and native numeric values when mapping Sentry fields`() {
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
        val capture = mock(HostProcessInitializerCapture::class.java)

        val result = captureHostProcessStartedIfEnabled(enabled = false, capture = capture)

        assertSuccess(result)
        verifyNoInteractions(capture)
    }

    @Test
    fun `returns failures for initializer logging when metadata or capture throws`() {
        val metadataError = IllegalStateException("native version unavailable")
        val metadataCapture = mock(HostProcessStartCapture::class.java)
        val metadataFailure = HostProcessTelemetry(
            appLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f"),
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { throw metadataError },
            capture = metadataCapture,
        )
        val captureError = IllegalStateException("Sentry unavailable")
        val capture = mock(HostProcessStartCapture::class.java)
        doThrow(captureError).`when`(capture).capture(any())
        val captureFailure = HostProcessTelemetry(
            appLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f"),
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { metadata },
            capture = capture,
        )

        val metadataResult = metadataFailure.captureProcessStarted()
        val captureResult = captureFailure.captureProcessStarted()

        assertSame(metadataError, metadataResult.exceptionOrNull())
        assertSame(captureError, captureResult.exceptionOrNull())
        verifyNoInteractions(metadataCapture)
    }

    @Test
    fun `reuses launch id when telemetry captures twice`() {
        val capture = mock(HostProcessStartCapture::class.java)
        val expectedLaunchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f")
        val telemetry = HostProcessTelemetry(
            appLaunchId = expectedLaunchId,
            processStartElapsedRealtime = { 1.milliseconds },
            metadata = { metadata },
            capture = capture,
        )

        telemetry.captureProcessStarted()
        telemetry.captureProcessStarted()

        val events = ArgumentCaptor.forClass(HostProcessStartEvent::class.java)
        verify(capture, times(2)).capture(events.capture())
        assertSame(events.allValues[0].appLaunchId, events.allValues[1].appLaunchId)
    }

    @Test
    fun `reports distribution flavor when build type is nightly`() {
        assertEquals("gpNightly", appVariant(flavor = "gp", buildType = "nightly"))
        assertEquals("vanillaNightly", appVariant(flavor = "vanilla", buildType = "nightly"))
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }
}
