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
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

class DeviceReadingsInitializer @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val batteryService: BatteryService,
) : AppInitializer {
    private val activityManager = context.getSystemService(ActivityManager::class.java)
    private val powerManager = context.getSystemService(PowerManager::class.java)
    private val coreCount = Runtime.getRuntime().availableProcessors()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        if (!Sentry.isEnabled()) return@runCatching

        scope.launch(Dispatchers.Default) {
            ProcessLifecycleOwner.get().repeatOnLifecycle(Lifecycle.State.STARTED) {
                var previousCpu: CpuSample? = null
                while (isActive) {
                    runCancellableCatching {
                        val cpu = CpuSample.now()
                        report(previousCpu?.let { cpu.usagePercentSince(it, coreCount) })
                        previousCpu = cpu
                    }.logFailure("Device readings tick failed")
                    delay(REPORT_INTERVAL)
                }
            }
        }
    }

    private fun report(processCpuUsage: Double?) {
        val metrics = Sentry.metrics()
        val memory = ActivityManager.MemoryInfo().also(activityManager::getMemoryInfo)
        val storage = StatFs(context.filesDir.path)

        processCpuUsage?.let { metrics.gauge("device.cpu.process_usage", it, MetricsUnit.Fraction.PERCENT) }
        metrics.gauge("device.cpu.cores", coreCount.toDouble())
        metrics.gauge("device.memory.total", memory.totalMem.toDouble(), MetricsUnit.Information.BYTE)
        metrics.gauge("device.memory.available", memory.availMem.toDouble(), MetricsUnit.Information.BYTE)
        metrics.gauge("device.memory.process", Debug.getPss() * 1024.0, MetricsUnit.Information.BYTE)
        metrics.gauge("device.storage.free", storage.availableBytes.toDouble(), MetricsUnit.Information.BYTE)
        metrics.gauge("device.storage.total", storage.totalBytes.toDouble(), MetricsUnit.Information.BYTE)
        metrics.gauge("device.thermal.state", powerManager.currentThermalStatus.toDouble())
        thermalHeadroom()?.let { metrics.gauge("device.thermal.headroom", it.toDouble()) }
        batteryService.currentBatteryLevel()?.let {
            metrics.gauge("device.battery.level", it.inPercents.toDouble(), MetricsUnit.Fraction.PERCENT)
        }
        metrics.gauge("device.battery.charging", if (batteryService.isCharging()) 1.0 else 0.0)
    }

    private fun thermalHeadroom(): Float? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return null

        return powerManager.getThermalHeadroom(0).takeUnless(Float::isNaN)
    }

    private companion object {
        val REPORT_INTERVAL = 1.minutes
    }
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
