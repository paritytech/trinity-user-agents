package io.paritytech.polkadotapp.app.monitoring

import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Locale

class DeviceReadingsTest {
    @Test
    fun `log line carries every reading with its unit`() {
        val readings = DeviceReadings(
            processCpuUsagePercent = 12.46,
            coreCount = 8,
            memoryTotalBytes = 5_678_900_000,
            memoryAvailableBytes = 1_234_500_000,
            processMemoryBytes = 210_400_000,
            storageFreeBytes = 12_345_600_000,
            storageTotalBytes = 123_456_700_000,
            thermalStatus = 1,
            thermalHeadroom = 0.437f,
            batteryLevelPercent = 81,
            charging = true,
        )

        assertEquals(
            "cpu=12.5% cores=8 memory=1234/5678MB process=210MB storage=12345/123456MB thermal=1 " +
                "headroom=0.44 battery=81% charging=true",
            readings.toLogLine(),
        )
    }

    @Test
    fun `log line writes decimals with a dot whatever the phone language`() {
        val readings = DeviceReadings(
            processCpuUsagePercent = 12.46,
            coreCount = 8,
            memoryTotalBytes = 5_678_900_000,
            memoryAvailableBytes = 1_234_500_000,
            processMemoryBytes = 210_400_000,
            storageFreeBytes = 12_345_600_000,
            storageTotalBytes = 123_456_700_000,
            thermalStatus = 1,
            thermalHeadroom = 0.437f,
            batteryLevelPercent = 81,
            charging = true,
        )
        val phoneLocale = Locale.getDefault()
        Locale.setDefault(Locale.GERMANY)

        val line = try {
            readings.toLogLine()
        } finally {
            Locale.setDefault(phoneLocale)
        }

        assertEquals(
            "cpu=12.5% cores=8 memory=1234/5678MB process=210MB storage=12345/123456MB thermal=1 " +
                "headroom=0.44 battery=81% charging=true",
            line,
        )
    }

    @Test
    fun `log line leaves out the readings the device could not take`() {
        val readings = DeviceReadings(
            processCpuUsagePercent = null,
            coreCount = 4,
            memoryTotalBytes = 4_000_000_000,
            memoryAvailableBytes = 1_000_000_000,
            processMemoryBytes = 150_000_000,
            storageFreeBytes = 20_000_000_000,
            storageTotalBytes = 64_000_000_000,
            thermalStatus = 0,
            thermalHeadroom = null,
            batteryLevelPercent = null,
            charging = false,
        )

        assertEquals(
            "cores=4 memory=1000/4000MB process=150MB storage=20000/64000MB thermal=0 charging=false",
            readings.toLogLine(),
        )
    }
}
