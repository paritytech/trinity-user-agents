package io.paritytech.polkadotapp.app.telemetry

import android.content.Intent
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.CriticalIssue
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventLevel
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.data.telemetry.isFreshNotificationLaunch
import io.paritytech.polkadotapp.test_shared.any
import io.sentry.SentryEvent
import io.sentry.SentryLevel
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
    private val sentryCapture: SentryEventCapture = mock(SentryEventCapture::class.java)

    @Test
    fun `captures the process event with the shared host tags when the app initializes`() {
        var captured: HostEvent? = null
        val initializer = initializer(capture = HostEventCapture { captured = it() })

        val result = with(computationalScope()) { initializer.initialize() }

        assertSuccess(result)
        val event = checkNotNull(captured)
        val launchId = event.extras["app.launch_id"] as String
        assertEquals(launchId, UUID.fromString(launchId).toString())
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
                    "app.launch_id" to launchId,
                    "event.monotonic_ms" to 1_234L,
                    "host.app_build" to 42L,
                ),
                level = HostEventLevel.INFO,
            ),
            event,
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
        )

        capture.capture { error("event metadata must stay lazy") }

        verifyNothingSentToSentry()
    }

    @Test
    fun `maps host event fields into one Sentry event when host telemetry is enabled`() {
        var sentryEvent: SentryEvent? = null
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { sentryEvent = it },
        )

        capture.capture {
            HostEvent(
                name = "host.process.started",
                tags = mapOf("host.platform" to "android"),
                extras = mapOf("host.app_build" to 42L),
                level = HostEventLevel.INFO,
            )
        }

        assertEquals("host.process.started", sentryEvent?.message?.formatted)
        assertEquals(SentryLevel.INFO, sentryEvent?.level)
        assertEquals("host.telemetry", sentryEvent?.logger)
        assertEquals(listOf("host.process.started"), sentryEvent?.fingerprints)
        assertEquals("android", sentryEvent?.getTag("host.platform"))
        assertEquals(42L, sentryEvent?.getExtra("host.app_build"))
    }

    @Test
    fun `sends each host event at the Sentry level of its own level`() {
        val levels = mutableListOf<SentryLevel?>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { levels += it.level },
        )

        HostEventLevel.entries.forEach { level ->
            capture.capture { HostEvent(name = "host.one", tags = emptyMap(), extras = emptyMap(), level = level) }
        }

        assertEquals(listOf(SentryLevel.INFO, SentryLevel.ERROR), levels)
    }

    @Test
    fun `captures an issue as one error event grouped on its flow and kind`() {
        var sentryEvent: SentryEvent? = null
        val capture = RealCriticalIssueCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            originProvider = HostEventOriginProvider { origin },
            sentryCapture = SentryEventCapture { sentryEvent = it },
        )

        capture.capture(
            CriticalIssue(flow = "product-notification", kind = "add-failed", error = SecurityException("alarms off")),
        )

        val event = checkNotNull(sentryEvent)
        assertEquals(
            listOf(
                "product-notification.add-failed",
                SentryLevel.ERROR,
                "host.telemetry",
                listOf("product-notification", "add-failed"),
                mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "host.os_version" to "15",
                    "flow" to "product-notification",
                    "kind" to "add-failed",
                    "errorType" to "java.lang.SecurityException",
                ),
                mapOf<String, Any>("host.app_build" to 42L),
            ),
            listOf(event.message?.formatted, event.level, event.logger, event.fingerprints, event.tags, event.extras),
        )
    }

    @Test
    fun `an issue without an error carries no error type`() {
        var sentryEvent: SentryEvent? = null
        val capture = RealCriticalIssueCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            originProvider = HostEventOriginProvider { origin },
            sentryCapture = SentryEventCapture { sentryEvent = it },
        )

        capture.capture(CriticalIssue(flow = "product-notification", kind = "store-failed", error = null))

        assertEquals(
            mapOf(
                "host.platform" to "android",
                "host.app_variant" to "nightly",
                "host.app_flavor" to "gp",
                "host.native_app_version" to "1.2.3",
                "host.os_version" to "15",
                "flow" to "product-notification",
                "kind" to "store-failed",
            ),
            sentryEvent?.tags,
        )
    }

    @Test
    fun `does not read the origin or capture an issue when host telemetry is disabled`() {
        val capture = RealCriticalIssueCapture(
            enabledProvider = HostTelemetryEnabledProvider { false },
            originProvider = HostEventOriginProvider { error("origin must stay unread") },
            sentryCapture = sentryCapture,
        )

        capture.capture(CriticalIssue(flow = "product-notification", kind = "add-failed", error = null))

        verifyNothingSentToSentry()
    }

    @Test
    fun `one event name is one Sentry group whatever else the event carries`() {
        val fingerprints = mutableListOf<List<String>?>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { fingerprints += it.fingerprints },
        )
        val otherBuild = origin.copy(appVariant = "debug", nativeAppVersion = "9.9.9", appBuild = 7L)

        capture.capture {
            HostEvent(
                name = "host.one",
                tags = origin.baseTags(),
                extras = origin.baseExtras(),
                level = HostEventLevel.INFO,
            )
        }
        capture.capture {
            HostEvent(
                name = "host.one",
                tags = otherBuild.baseTags(),
                extras = otherBuild.baseExtras(),
                level = HostEventLevel.INFO,
            )
        }
        capture.capture {
            HostEvent(name = "host.two", tags = emptyMap(), extras = emptyMap(), level = HostEventLevel.INFO)
        }

        assertEquals(listOf(listOf("host.one"), listOf("host.one"), listOf("host.two")), fingerprints)
    }

    @Test
    fun `records a product notification tap only on a fresh launch`() {
        assertTrue(isFreshNotificationLaunch(isRestored = false, intentFlags = Intent.FLAG_ACTIVITY_NEW_TASK))
        assertFalse(isFreshNotificationLaunch(isRestored = true, intentFlags = 0))
        assertFalse(
            isFreshNotificationLaunch(
                isRestored = false,
                intentFlags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY,
            )
        )
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

    private fun computationalScope() = ComputationalScope(CoroutineScope(Dispatchers.Unconfined))

    private fun verifyNothingSentToSentry() {
        verifyNoInteractions(sentryCapture)
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }
}
