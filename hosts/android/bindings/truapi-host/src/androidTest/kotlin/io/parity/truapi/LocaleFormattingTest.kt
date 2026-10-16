package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostLocaleLocalizeTimestampsRequest
import uniffi.truapi.HostRejection
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.RemotePermission

@RunWith(AndroidJUnit4::class)
class LocaleFormattingTest {
    private val bridge = object : HostBridge {
        override fun permissionAuthorizationsChanged(productId: String) = Unit
        override val storage: HostStorage get() = error("Formatting must not access storage")
        override val coreStorage: HostCoreStorage get() = error("Formatting must not access core storage")
        override suspend fun navigateTo(url: String) = Unit
        override suspend fun devicePermission(product: ProductExecutionConfig, request: HostDevicePermissionRequest) =
            PermissionDecision.DENY
        override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission) =
            PermissionDecision.DENY
        override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false
    }

    @Test
    fun timestampsUseDSTAndLocalMidnightAndNewContext() = runBlocking {
        val instants = listOf(
            "2024-03-10T06:59:00Z", "2024-03-10T07:01:00Z",
            "2024-03-10T04:59:00Z", "2024-03-10T05:01:00Z",
            "2024-11-03T05:30:00Z", "2024-11-03T06:30:00Z",
        ).map(Instant::parse)
        val timestamps = instants.map { it.toEpochMilli().toULong() }
        val response = bridge.localizeTimestamps(
            HostLocaleLocalizeTimestampsRequest(timestamps, "en-US", "America/New_York"),
        ).timestamps
        assertEquals("2024-03-09", response[2].localDate)
        assertEquals("2024-03-10", response[3].localDate)
        val expected = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT)
            .withLocale(Locale.US).withZone(ZoneId.of("America/New_York"))
        assertEquals(expected.format(instants[0]), response[0].time)
        assertEquals(expected.format(instants[1]), response[1].time)
        assertEquals(response[4].time, response[5].time)
        assertNotEquals(response[4].dateTime, response[5].dateTime)
        val changed = bridge.localizeTimestamps(
            HostLocaleLocalizeTimestampsRequest(timestamps, "fr-FR", "Europe/Paris"),
        ).timestamps
        assertEquals("2024-03-10", changed[2].localDate)
        assertNotEquals(response[0].date, changed[0].date)
    }

    @Test
    fun dateKeysStayGregorianAndUnknownZonesAreRejected() = runBlocking {
        val response = bridge.localizeTimestamps(
            HostLocaleLocalizeTimestampsRequest(listOf(0uL), "th-TH-u-ca-buddhist", "Asia/Bangkok"),
        )
        assertEquals("1970-01-01", response.timestamps.single().localDate)
        assertThrows(HostRejection.Rejected::class.java) {
            runBlocking {
                bridge.localizeTimestamps(
                    HostLocaleLocalizeTimestampsRequest(listOf(0uL), "en-US", "Not/AZone"),
                )
            }
        }
        Unit
    }
}
