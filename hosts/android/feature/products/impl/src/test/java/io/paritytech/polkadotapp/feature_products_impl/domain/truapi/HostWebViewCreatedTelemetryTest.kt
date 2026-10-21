package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

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
        var captured: HostWebViewCreatedEvent? = null
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            packageName = "com.example.nightly",
            metadata = { metadata },
            capture = { captured = it },
        )

        reporter.report("bootstrap-session-42")

        assertEquals("bootstrap-session-42", captured?.sessionId)
    }

    @Test
    fun releaseSkipsMetadataAndCapture() {
        var metadataRead = false
        var captured = false
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            packageName = "com.example",
            metadata = { metadataRead = true; metadata },
            capture = { captured = true },
        )

        reporter.report("release-session")

        assertTrue(!metadataRead)
        assertTrue(!captured)
    }

    @Test
    fun eventBuildAndMonotonicTimeRemainNumeric() {
        var captured: HostWebViewCreatedEvent? = null
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            packageName = "com.example.debug",
            metadata = { metadata },
            capture = { captured = it },
        )

        reporter.report("session")

        assertEquals(123L, captured?.appBuild)
        assertEquals(456L, captured?.monotonicMs)
    }

    @Test
    fun captureFailureDoesNotEscape() {
        val reporter = HostWebViewCreatedTelemetry.createForTest(
            packageName = "com.example.safetynet",
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
