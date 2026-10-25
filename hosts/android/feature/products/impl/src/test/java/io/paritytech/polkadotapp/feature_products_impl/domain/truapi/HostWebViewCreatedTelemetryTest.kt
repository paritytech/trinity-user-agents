package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.sentry.SentryEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HostWebViewCreatedTelemetryTest {
    private val packageInfo = mockk<PackageInfo> {
        versionName = "1.2.3"
        every { longVersionCode } returns 123L
    }
    private val packageManager = mockk<PackageManager> {
        every { getPackageInfo(PACKAGE_NAME, 0) } returns packageInfo
    }
    private val context = mockk<Context>()
    private val timeProvider = mockk<HostMonotonicTimeProvider> {
        every { nowMillis() } returns 456L
    }

    init {
        every { context.packageName } returns PACKAGE_NAME
        every { context.packageManager } returns packageManager
    }

    @Test
    fun `event reports the bootstrap session id`() {
        var captured: SentryEvent? = null
        val reporter = HostWebViewCreatedTelemetry(
            context = context,
            timeProvider = timeProvider,
            capture = HostWebViewCreatedEventCapture { event -> captured = event() },
        )

        reporter.report("bootstrap-session-42")

        assertEquals("bootstrap-session-42", captured?.getExtra("host.session_id"))
    }

    @Test
    fun `metadata remains lazy when capture is disabled`() {
        val reporter = HostWebViewCreatedTelemetry(
            context = context,
            timeProvider = timeProvider,
            capture = HostWebViewCreatedEventCapture { },
        )

        reporter.report("release-session")

        verify(exactly = 0) { context.packageManager }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    @Test
    fun `event build and monotonic time remain numeric`() {
        var captured: SentryEvent? = null
        val reporter = HostWebViewCreatedTelemetry(
            context = context,
            timeProvider = timeProvider,
            capture = HostWebViewCreatedEventCapture { event -> captured = event() },
        )

        reporter.report("session")

        assertEquals(HostWebViewCreatedTelemetry.EVENT_NAME, captured?.message?.formatted)
        assertEquals(123L, captured?.getExtra("host.app_build"))
        assertEquals(456L, captured?.getExtra("event.monotonic_ms"))
        assertTrue(captured?.getExtra("host.app_build") is Long)
        assertTrue(captured?.getExtra("event.monotonic_ms") is Long)
    }

    @Test
    fun `capture failure does not escape`() {
        val capture = mockk<HostWebViewCreatedEventCapture>()
        every { capture.capture(any()) } throws IllegalStateException("Sentry unavailable")
        val reporter = HostWebViewCreatedTelemetry(
            context = context,
            timeProvider = timeProvider,
            capture = capture,
        )

        reporter.report("session")

        verify(exactly = 1) { capture.capture(any()) }
    }

    private companion object {
        const val PACKAGE_NAME = "io.paritytech.polkadotapp.nightly"
    }
}
