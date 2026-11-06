package io.paritytech.polkadotapp.app.telemetry

import io.paritytech.polkadotapp.app.monitoring.SentryMetricsCapture
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

class HostProcessTelemetryTest {
    private val origin = HostEventOrigin(
        appVariant = "nightly",
        appFlavor = "gp",
        nativeAppVersion = "1.2.3",
        platform = "android",
        osVersion = "15",
        appBuild = 42L,
        launchId = LAUNCH_ID,
    )
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
                    "app.launch_id" to LAUNCH_ID,
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
        val initializer = HostProcessTelemetryInitializer(
            originProvider = HostEventOriginProvider {
                originRead = true
                origin
            },
            timeProvider = HostMonotonicTimeProvider {
                timeRead = true
                1_234L
            },
            capture = HostEventCapture { },
        )

        val result = with(computationalScope()) { initializer.initialize() }

        assertSuccess(result)
        assertFalse(originRead)
        assertFalse(timeRead)
    }

    @Test
    fun `does not create or capture an event when host telemetry is disabled`() {
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { false },
            sentryCapture = sentryCapture,
            metrics = metrics { _, _, _, _ -> error("no distribution while host telemetry is off") },
        )

        capture.capture { error("event metadata must stay lazy") }

        verifyNothingSentToSentry()
        assertFalse(capture.enabled)
    }

    @Test
    fun `maps host event fields into one Sentry event when host telemetry is enabled`() {
        var sentryEvent: SentryEvent? = null
        val distributions = mutableListOf<Any>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { sentryEvent = it },
            metrics = metrics { name, value, unit, attributes -> distributions += listOf(name, value, unit, attributes) },
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
        assertEquals(emptyList<Any>(), distributions)
    }

    // Sentry cannot chart a number an event carries, so a step's duration goes out a second time as
    // a distribution, under the event's name and filterable by the same tags. A distribution is
    // aggregated, so it leaves out the session, which is unique to each open.
    @Test
    fun `sends a step's duration as a distribution under the event's name and tags but the session`() {
        val distributions = mutableListOf<Any>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { },
            metrics = metrics { name, value, unit, attributes -> distributions += listOf(name, value, unit, attributes) },
        )
        val tags = mapOf("outcome" to "ok", "host.session_id" to "session-1")

        capture.capture {
            HostEvent(name = "host.worker.boot", tags = tags, extras = emptyMap(), durationMs = 850L)
        }

        assertEquals(listOf(listOf("host.worker.boot", 850.0, "millisecond", mapOf("outcome" to "ok"))), distributions)
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

    private fun initializer(capture: HostEventCapture) = HostProcessTelemetryInitializer(
        originProvider = HostEventOriginProvider { origin },
        timeProvider = HostMonotonicTimeProvider { 1_234L },
        capture = capture,
    )

    private fun metrics(distribution: (String, Double, String?, Map<String, String>) -> Unit) =
        object : SentryMetricsCapture {
            override fun gauge(key: String, value: Double, unit: String?) = error("host events send no gauges")

            override fun distribution(key: String, value: Double, unit: String?, attributes: Map<String, String>) =
                distribution(key, value, unit, attributes)
        }

    private fun computationalScope() = ComputationalScope(CoroutineScope(Dispatchers.Unconfined))

    private companion object {
        const val LAUNCH_ID = "0f8fad5b-d9cb-469f-a165-70867728950e"
    }

    private fun verifyNothingSentToSentry() {
        verifyNoInteractions(sentryCapture)
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }
}
