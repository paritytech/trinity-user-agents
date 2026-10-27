package io.paritytech.polkadotapp.app.telemetry

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.test_shared.any
import io.sentry.SentryEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions
import java.util.UUID

class HostProcessTelemetryTest {
    private val origin = HostEventOrigin(
        appVariant = "nightly",
        appFlavor = "gp",
        nativeAppVersion = "1.2.3",
        platform = "android",
        osVersion = "15",
        appBuild = 42L,
    )
    private val launchId = UUID.fromString("608ad8ee-51f5-49ab-af80-e25fd72c434f")
    private val sentryCapture: SentryEventCapture = mock(SentryEventCapture::class.java)

    @Test
    fun `captures the process event with the shared host tags when the app initializes`() {
        var captured: HostEvent? = null
        val initializer = initializer(capture = HostEventCapture { captured = it() })

        val result = with(computationalScope()) { initializer.initialize() }

        assertSuccess(result)
        assertEquals(
            HostEvent(
                name = "host.process.started",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "host.os_version" to "15",
                ),
                extras = mapOf(
                    "app.launch_id" to "608ad8ee-51f5-49ab-af80-e25fd72c434f",
                    "event.monotonic_ms" to 1_234L,
                    "host.app_build" to 42L,
                ),
            ),
            captured,
        )
    }

    @Test
    fun `keeps process metadata lazy when capture is disabled`() {
        var originRead = false
        var timeRead = false
        var launchIdRead = false
        val initializer = AndroidHostProcessTelemetryInitializer(
            originProvider = HostEventOriginProvider {
                originRead = true
                origin
            },
            timeProvider = HostMonotonicTimeProvider {
                timeRead = true
                1_234L
            },
            launchIdProvider = HostLaunchIdProvider {
                launchIdRead = true
                launchId
            },
            capture = HostEventCapture { },
        )

        val result = with(computationalScope()) { initializer.initialize() }

        assertSuccess(result)
        assertFalse(originRead)
        assertFalse(timeRead)
        assertFalse(launchIdRead)
    }

    @Test
    fun `does not create or capture an event when host telemetry is disabled`() {
        val capture = SentryHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { false },
            sentryCapture = sentryCapture,
        )

        capture.capture { error("event metadata must stay lazy") }

        verifyNothingSentToSentry()
    }

    @Test
    fun `maps host event fields into one Sentry event when host telemetry is enabled`() {
        var sentryEvent: SentryEvent? = null
        val capture = SentryHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { sentryEvent = it },
        )

        capture.capture {
            HostEvent(
                name = "host.process.started",
                tags = mapOf("host.platform" to "android"),
                extras = mapOf("host.app_build" to 42L),
            )
        }

        assertEquals("host.process.started", sentryEvent?.message?.formatted)
        assertEquals("android", sentryEvent?.getTag("host.platform"))
        assertEquals(42L, sentryEvent?.getExtra("host.app_build"))
    }

    @Test
    fun `returns the capture failure when capture throws`() {
        val error = IllegalStateException("Sentry unavailable")
        val capture = mock(HostEventCapture::class.java)
        doThrow(error).`when`(capture).capture(any())
        val initializer = initializer(capture)

        val result = with(computationalScope()) { initializer.initialize() }

        assertSame(error, result.exceptionOrNull())
    }

    private fun initializer(capture: HostEventCapture) = AndroidHostProcessTelemetryInitializer(
        originProvider = HostEventOriginProvider { origin },
        timeProvider = HostMonotonicTimeProvider { 1_234L },
        launchIdProvider = HostLaunchIdProvider { launchId },
        capture = capture,
    )

    private fun computationalScope() = ComputationalScope(CoroutineScope(Dispatchers.Unconfined))

    private fun verifyNothingSentToSentry() {
        verifyNoInteractions(sentryCapture)
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }
}
