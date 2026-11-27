package io.paritytech.polkadotapp.app.logging

import android.app.Application
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Bundle
import io.mockk.every
import io.mockk.mockk
import io.sentry.SentryLogEvent
import io.sentry.SentryLogEventAttributeValue
import io.sentry.SentryLogLevel
import io.sentry.protocol.SentryId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SentryLogGateTest {
    @Test
    fun `the gate sends only the logs the filter admits`() {
        val callback = SentryLogGate.beforeSendLog(HostLogFilter { 0 })
        val ours = logEvent(body = "connection 1 closing", tag = "truapi.chain")

        assertNull(callback.execute(logEvent(body = "peer lost", tag = "DeviceSync")))
        assertNull(callback.execute(logEvent(body = "peer lost", tag = null)))
        assertEquals(ours, callback.execute(ours))
        assertNull(ours.attributes?.get(HostLogFilter.DROPPED_ATTRIBUTE))
    }

    @Test
    fun `the first log after a minute that dropped some carries the count`() {
        var now = 0L
        val callback = SentryLogGate.beforeSendLog(HostLogFilter { now })
        repeat(HostLogFilter.LINES_PER_WINDOW + 7) {
            callback.execute(logEvent(body = "follow p:$it restarted", tag = "truapi.chain"))
        }

        now += HostLogFilter.WINDOW_MS
        val carried = callback.execute(logEvent(body = "follow p:9999 restarted", tag = "truapi.chain"))

        val dropped = carried?.attributes?.get(HostLogFilter.DROPPED_ATTRIBUTE)
        assertEquals(7, dropped?.value)
        assertEquals("integer", dropped?.type)
    }

    @Test
    fun `a build starts sentry only when its manifest carries a dsn`() {
        assertFalse(SentryLogGate.startsSentry(application(dsn = null)))
        assertFalse(SentryLogGate.startsSentry(application(dsn = "")))
        assertTrue(SentryLogGate.startsSentry(application(dsn = "https://key@example.invalid/1")))
    }

    @Test
    fun `a package manager that cannot answer leaves the build dark`() {
        val packages = mockk<PackageManager>()
        every { packages.getApplicationInfo(any<String>(), any<Int>()) } throws
            PackageManager.NameNotFoundException()
        val application = mockk<Application>().also {
            every { it.packageManager } returns packages
            every { it.packageName } returns "io.example.localtest"
        }

        assertFalse(SentryLogGate.startsSentry(application))
    }

    private fun logEvent(body: String, tag: String?): SentryLogEvent {
        val event = SentryLogEvent(SentryId.EMPTY_ID, 0.0, body, SentryLogLevel.WARN)
        if (tag != null) {
            event.setAttribute(
                SentryHostLogSink.TAG_ATTRIBUTE,
                SentryLogEventAttributeValue("string", tag),
            )
        }
        return event
    }

    private fun application(dsn: String?): Application {
        val metaData = mockk<Bundle>()
        every { metaData.getString("io.sentry.dsn") } returns dsn
        val info = ApplicationInfo().apply { this.metaData = metaData }
        val packages = mockk<PackageManager>()
        every { packages.getApplicationInfo(any<String>(), any<Int>()) } returns info
        return mockk<Application>().also {
            every { it.packageManager } returns packages
            every { it.packageName } returns "io.example.localtest"
        }
    }
}
