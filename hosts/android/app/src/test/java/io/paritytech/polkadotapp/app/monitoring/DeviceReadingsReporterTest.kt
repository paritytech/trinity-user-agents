package io.paritytech.polkadotapp.app.monitoring

import io.paritytech.polkadotapp.app.telemetry.HostTelemetryEnabledProvider
import org.junit.Assert.assertEquals
import org.junit.Test

class DeviceReadingsReporterTest {
    private val readings = DeviceReadings(
        processCpuUsagePercent = 12.5,
        coreCount = 8,
        memoryTotalBytes = 5_678_900_000,
        memoryAvailableBytes = 1_234_500_000,
        processMemoryBytes = 210_400_000,
        storageFreeBytes = 12_345_600_000,
        storageTotalBytes = 123_456_700_000,
        thermalStatus = 1,
        thermalHeadroom = 0.4f,
        batteryLevelPercent = 81,
        charging = true,
    )
    private val sentKeys = mutableListOf<String>()
    private val metrics = SentryMetricsCapture { key, _, _ -> sentKeys += key }

    @Test
    fun `totals that cannot change while the process runs go out once across reports`() {
        // Given
        val reporter = DeviceReadingsReporter(HostTelemetryEnabledProvider { true }, metrics)

        // When
        reporter.report(readings)
        reporter.report(readings)

        // Then
        assertEquals(
            mapOf(
                "device.cpu.cores" to 1,
                "device.memory.total" to 1,
                "device.storage.total" to 1,
                "device.cpu.process_usage" to 2,
                "device.memory.available" to 2,
                "device.memory.process" to 2,
                "device.storage.free" to 2,
                "device.thermal.state.android" to 2,
                "device.thermal.headroom" to 2,
                "device.battery.level" to 2,
                "device.battery.charging" to 2,
            ),
            sentKeys.groupingBy { it }.eachCount(),
        )
    }

    @Test
    fun `sends no gauge when host telemetry is off`() {
        // Given
        val reporter = DeviceReadingsReporter(HostTelemetryEnabledProvider { false }, metrics)

        // When
        reporter.report(readings)

        // Then
        assertEquals(emptyList<String>(), sentKeys)
    }
}
