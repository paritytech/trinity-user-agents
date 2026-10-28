package io.paritytech.polkadotapp.app.monitoring

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.os.Debug
import android.os.PowerManager
import android.os.Process
import android.os.StatFs
import android.os.SystemClock
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.telemetry.HostTelemetryEnabledProvider
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.platform.BatteryService
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.sentry.Sentry
import io.sentry.metrics.MetricsUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.Locale
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

internal class DeviceReadingsInitializer @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val batteryService: BatteryService,
    private val reporter: DeviceReadingsReporter,
) : AppInitializer {
    private val activityManager = context.getSystemService(ActivityManager::class.java)
    private val powerManager = context.getSystemService(PowerManager::class.java)
    private val coreCount = Runtime.getRuntime().availableProcessors()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        scope.launch(Dispatchers.Default) {
            ProcessLifecycleOwner.get().repeatOnLifecycle(Lifecycle.State.STARTED) {
                var previousCpu: CpuSample? = null
                while (isActive) {
                    runCancellableCatching {
                        val cpu = CpuSample.now()
                        val readings = read(previousCpu?.let { cpu.usagePercentSince(it, coreCount) })
                        Timber.tag(DEVICE_READINGS_LOG_TAG).i("Device readings: %s", readings.toLogLine())
                        reporter.report(readings)
                        previousCpu = cpu
                    }.logFailure("Device readings tick failed")
                    delay(REPORT_INTERVAL)
                }
            }
        }
    }

    private fun read(processCpuUsagePercent: Double?): DeviceReadings {
        val memory = ActivityManager.MemoryInfo().also(activityManager::getMemoryInfo)
        val storage = StatFs(context.filesDir.path)

        return DeviceReadings(
            processCpuUsagePercent = processCpuUsagePercent,
            coreCount = coreCount,
            memoryTotalBytes = memory.totalMem,
            memoryAvailableBytes = memory.availMem,
            processMemoryBytes = Debug.getPss() * 1024,
            storageFreeBytes = storage.availableBytes,
            storageTotalBytes = storage.totalBytes,
            thermalStatus = powerManager.currentThermalStatus,
            thermalHeadroom = thermalHeadroom(),
            batteryLevelPercent = batteryService.currentBatteryLevel()?.inPercents?.toInt(),
            charging = batteryService.isCharging(),
        )
    }

    private fun thermalHeadroom(): Float? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return null

        return powerManager.getThermalHeadroom(0).takeUnless(Float::isNaN)
    }

    private companion object {
        val REPORT_INTERVAL = 1.minutes
    }
}

internal fun interface SentryMetricsCapture {
    fun gauge(key: String, value: Double, unit: String?)
}

internal class RealSentryMetricsCapture @Inject constructor() : SentryMetricsCapture {
    override fun gauge(key: String, value: Double, unit: String?) {
        Sentry.metrics().gauge(key, value, unit)
    }
}

internal class DeviceReadingsReporter @Inject constructor(
    private val telemetryEnabledProvider: HostTelemetryEnabledProvider,
    private val metrics: SentryMetricsCapture,
) {
    private var launchConstantsReported = false

    fun report(readings: DeviceReadings) {
        if (!telemetryEnabledProvider.isEnabled()) return

        with(readings) {
            if (!launchConstantsReported) {
                metrics.gauge("device.cpu.cores", coreCount.toDouble(), null)
                metrics.gauge("device.memory.total", memoryTotalBytes.toDouble(), MetricsUnit.Information.BYTE)
                metrics.gauge("device.storage.total", storageTotalBytes.toDouble(), MetricsUnit.Information.BYTE)
                launchConstantsReported = true
            }
            processCpuUsagePercent?.let { metrics.gauge("device.cpu.process_usage", it, MetricsUnit.Fraction.PERCENT) }
            metrics.gauge("device.memory.available", memoryAvailableBytes.toDouble(), MetricsUnit.Information.BYTE)
            metrics.gauge("device.memory.process", processMemoryBytes.toDouble(), MetricsUnit.Information.BYTE)
            metrics.gauge("device.storage.free", storageFreeBytes.toDouble(), MetricsUnit.Information.BYTE)
            metrics.gauge("device.thermal.state.android", thermalStatus.toDouble(), null)
            thermalHeadroom?.let { metrics.gauge("device.thermal.headroom", it.toDouble(), null) }
            batteryLevelPercent?.let {
                metrics.gauge("device.battery.level", it.toDouble(), MetricsUnit.Fraction.PERCENT)
            }
            metrics.gauge("device.battery.charging", if (charging) 1.0 else 0.0, null)
        }
    }
}

internal data class DeviceReadings(
    val processCpuUsagePercent: Double?,
    val coreCount: Int,
    val memoryTotalBytes: Long,
    val memoryAvailableBytes: Long,
    val processMemoryBytes: Long,
    val storageFreeBytes: Long,
    val storageTotalBytes: Long,
    val thermalStatus: Int,
    val thermalHeadroom: Float?,
    val batteryLevelPercent: Int?,
    val charging: Boolean,
) {
    fun toLogLine(): String = listOfNotNull(
        processCpuUsagePercent?.let { "cpu=${"%.1f".format(Locale.ROOT, it)}%" },
        "cores=$coreCount",
        "memory=${memoryAvailableBytes.megabytes}/${memoryTotalBytes.megabytes}MB",
        "process=${processMemoryBytes.megabytes}MB",
        "storage=${storageFreeBytes.megabytes}/${storageTotalBytes.megabytes}MB",
        "thermal=$thermalStatus",
        thermalHeadroom?.let { "headroom=${"%.2f".format(Locale.ROOT, it)}" },
        batteryLevelPercent?.let { "battery=$it%" },
        "charging=$charging",
    ).joinToString(" ")

    private val Long.megabytes get() = this / 1_000_000
}

internal class CpuSample(
    val processCpuMillis: Long,
    val elapsedRealtimeMillis: Long,
) {
    fun usagePercentSince(previous: CpuSample, coreCount: Int): Double {
        val wallMillis = (elapsedRealtimeMillis - previous.elapsedRealtimeMillis) * coreCount
        return (processCpuMillis - previous.processCpuMillis) * 100.0 / wallMillis
    }

    companion object {
        fun now() = CpuSample(Process.getElapsedCpuTime(), SystemClock.elapsedRealtime())
    }
}
