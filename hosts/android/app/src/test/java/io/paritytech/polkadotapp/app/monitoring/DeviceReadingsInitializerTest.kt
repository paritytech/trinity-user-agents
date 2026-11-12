package io.paritytech.polkadotapp.app.monitoring

import android.content.Context
import io.paritytech.polkadotapp.app.telemetry.HostTelemetryEnabledProvider
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.platform.BatteryService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class DeviceReadingsInitializerTest {
    @Test
    fun `starts no reading loop when host telemetry is off`() {
        // Given
        val telemetryOff = HostTelemetryEnabledProvider { false }
        val initializer = DeviceReadingsInitializer(
            context = mock(Context::class.java),
            batteryService = mock(BatteryService::class.java),
            reporter = DeviceReadingsReporter(telemetryOff) { _, _, _ -> },
            telemetryEnabledProvider = telemetryOff,
        )
        val parentJob = Job()

        // When
        val result = with(ComputationalScope(CoroutineScope(parentJob + Dispatchers.Unconfined))) {
            initializer.initialize()
        }

        // Then
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
        assertEquals(emptyList<Job>(), parentJob.children.toList())
        assertTrue("a launched loop that failed would have cancelled the scope", parentJob.isActive)
    }
}
