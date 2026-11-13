package io.paritytech.polkadotapp.app.telemetry

import android.content.Intent
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.data.telemetry.isFreshNotificationLaunch
import io.paritytech.polkadotapp.common.data.telemetry.productNotificationShown
import io.paritytech.polkadotapp.common.data.telemetry.productNotificationTapped
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
    private val logCapture: SentryLogCapture = mock(SentryLogCapture::class.java)

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
                isFailure = false,
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
            logCapture = logCapture,
        )

        capture.capture { error("event metadata must stay lazy") }

        verifyNothingSentToSentry()
    }

    @Test
    fun `sends an event that is not a failure to Sentry Logs with its tags and extras, and raises no issue`() {
        val logs = mutableListOf<Pair<String, Map<String, Any>>>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = sentryCapture,
            logCapture = SentryLogCapture { message, attributes -> logs += message to attributes },
        )

        capture.capture {
            HostEvent(
                name = "host.webview.created",
                tags = mapOf("host.platform" to "android", "host.session_id" to "session-1"),
                extras = mapOf("host.app_build" to 42L, "tap_ms" to 141L),
                isFailure = false,
            )
        }

        verifyNoInteractions(sentryCapture)
        assertEquals(
            listOf(
                "host.webview.created" to mapOf<String, Any>(
                    "host.platform" to "android",
                    "host.session_id" to "session-1",
                    "host.app_build" to 42L,
                    "tap_ms" to 141L,
                    "tag" to "host.telemetry",
                    "event.name" to "host.webview.created",
                ),
            ),
            logs,
        )
    }

    @Test
    fun `a tag keeps its value on the log when an extra has the same name`() {
        val event = HostEvent(
            name = "host.page.visible",
            tags = mapOf("host.session_id" to "from-tag"),
            extras = mapOf("host.session_id" to "from-extra", "webview_ms" to 781L),
            isFailure = false,
        )

        val attributes = event.toLogAttributes()

        assertEquals("from-tag", attributes["host.session_id"])
        assertEquals(781L, attributes["webview_ms"])
    }

    @Test
    fun `maps a failure into one Sentry error event and sends no log`() {
        var sentryEvent: SentryEvent? = null
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { sentryEvent = it },
            logCapture = logCapture,
        )

        capture.capture {
            HostEvent(
                name = "host.load.failed",
                tags = mapOf("host.platform" to "android"),
                extras = mapOf("host.app_build" to 42L),
                isFailure = true,
            )
        }

        verifyNoInteractions(logCapture)
        assertEquals("host.load.failed", sentryEvent?.message?.formatted)
        assertEquals(SentryLevel.ERROR, sentryEvent?.level)
        assertEquals("host.telemetry", sentryEvent?.logger)
        assertEquals(listOf("host.load.failed"), sentryEvent?.fingerprints)
        assertEquals("android", sentryEvent?.getTag("host.platform"))
        assertEquals(42L, sentryEvent?.getExtra("host.app_build"))
    }

    @Test
    fun `one failure name is one Sentry group whatever else the event carries`() {
        val fingerprints = mutableListOf<List<String>?>()
        val capture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { true },
            sentryCapture = SentryEventCapture { fingerprints += it.fingerprints },
            logCapture = logCapture,
        )
        val otherBuild = origin.copy(appVariant = "debug", nativeAppVersion = "9.9.9", appBuild = 7L)

        capture.capture { HostEvent("host.one", origin.baseTags(), origin.baseExtras(), isFailure = true) }
        capture.capture {
            HostEvent("host.one", otherBuild.baseTags(), otherBuild.baseExtras(), isFailure = true)
        }
        capture.capture { HostEvent("host.two", emptyMap(), emptyMap(), isFailure = true) }

        assertEquals(listOf(listOf("host.one"), listOf("host.one"), listOf("host.two")), fingerprints)
    }

    @Test
    fun `captures product notification fields and keeps disabled capture lazy`() {
        val captured = mutableListOf<HostEvent>()
        val capture = HostEventCapture { captured += it() }

        capture.capture { origin.productNotificationShown(productId = "jolly.dot") }
        capture.capture { origin.productNotificationTapped(productId = "jolly.dot") }

        val expectedTags = mapOf(
            "host.platform" to "android",
            "host.app_variant" to "nightly",
            "host.app_flavor" to "gp",
            "host.native_app_version" to "1.2.3",
            "product_notification.product_id" to "jolly.dot",
        )
        val expectedExtras = mapOf<String, Any>("host.app_build" to 42L)
        assertEquals(
            listOf(
                HostEvent("host.product_notification.shown", expectedTags, expectedExtras, isFailure = false),
                HostEvent("host.product_notification.tapped", expectedTags, expectedExtras, isFailure = false),
            ),
            captured,
        )

        var eventBuilt = false
        val disabledCapture = RealHostEventCapture(
            enabledProvider = HostTelemetryEnabledProvider { false },
            sentryCapture = sentryCapture,
            logCapture = logCapture,
        )
        disabledCapture.capture {
            eventBuilt = true
            origin.productNotificationShown(productId = "jolly.dot")
        }

        assertFalse(eventBuilt)
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
        verifyNoInteractions(sentryCapture, logCapture)
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }
}
