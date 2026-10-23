package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.sentry.SentryEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HostWebViewCreatedTelemetryTest {
    private val metadata = HostWebViewCreatedEvent(
        sessionId = "",
        appVariant = "nightly",
        nativeAppVersion = "1.2.3",
        hostVersion = "0.2.13",
        appBuild = 123L,
        monotonicMs = 456L,
    )

    @Test
    fun reportsTheBootstrapSessionId() {
        var captured: SentryEvent? = null
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            enabled = { true },
            metadata = { metadata },
            capture = { captured = it },
        )

        reporter.report("bootstrap-session-42")

        assertEquals("bootstrap-session-42", captured?.getExtra("host.session_id"))
    }

    @Test
    fun releaseSkipsMetadataAndCapture() {
        var metadataRead = false
        var captured = false
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            enabled = { false },
            metadata = { metadataRead = true; metadata },
            capture = { captured = true },
        )

        reporter.report("release-session")

        assertTrue(!metadataRead)
        assertTrue(!captured)
    }

    @Test
    fun eventBuildAndMonotonicTimeRemainNumeric() {
        var captured: SentryEvent? = null
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            enabled = { true },
            metadata = { metadata },
            capture = { captured = it },
        )

        reporter.report("session")

        assertEquals(HostWebViewCreatedTelemetry.EVENT_NAME, captured?.message?.formatted)
        assertEquals(123L, captured?.getExtra("host.app_build"))
        assertEquals(456L, captured?.getExtra("event.monotonic_ms"))
        assertTrue(captured?.getExtra("host.app_build") is Long)
        assertTrue(captured?.getExtra("event.monotonic_ms") is Long)
    }

    @Test
    fun captureFailureDoesNotEscape() {
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            enabled = { true },
            metadata = { metadata },
            capture = { error("Sentry unavailable") },
        )

        reporter.report("session")
    }

    @Test
    fun eachWebViewSessionGetsADistinctId() {
        assertNotEquals(
            HostWebViewCreatedTelemetry.newSessionId(),
            HostWebViewCreatedTelemetry.newSessionId(),
        )
    }

}
