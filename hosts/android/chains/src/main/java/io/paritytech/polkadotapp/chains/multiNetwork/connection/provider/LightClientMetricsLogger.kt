package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.Locale
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

private val LOG_INTERVAL = 1.minutes
private const val LOG_TAG = "LightClientMetrics"

/**
 * Logs the light client's network counters every [LOG_INTERVAL] while the app is in the foreground,
 * in the same lines as truapi-provider's `network_usage` example, so phone and desktop runs compare
 * directly: the process's bytes and connections (with the change since the last line), then each
 * chain's bytes per protocol, largest first. Counters run from the light client's start.
 */
class LightClientMetricsLogger @Inject constructor(
    private val lightClient: LightClient,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val dispatchers: CoroutineDispatchers,
) : AppInitializer {
    private var previous: LightClientMetricsReading? = null

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        if (!lightClient.servesAny()) return@runCatching

        scope.launch(dispatchers.computation) {
            appLifecycleObserver.subscribeIsForeground().collectLatest { foreground ->
                while (foreground) {
                    delay(LOG_INTERVAL)
                    log(lightClient.metrics())
                }
            }
        }
    }

    private fun log(answers: Map<String, String>) {
        val readings = answers.mapValues { (_, answer) -> LightClientMetricsReading.parse(answer) }
        readings.values.firstOrNull()?.let { process ->
            val last = previous
            fun change(now: Double, before: Double?) = before?.let { " (+%.2f)".format(Locale.ROOT, mb(now - it)) }.orEmpty()
            Timber.tag(LOG_TAG).i(
                "smoldot: received=%.2fMB%s sent=%.2fMB%s connectionsDialed=%.0f handshakesFinished=%.0f",
                mb(process.bytesReceived), change(process.bytesReceived, last?.bytesReceived),
                mb(process.bytesSent), change(process.bytesSent, last?.bytesSent),
                process.connectionsDialed, process.handshakesFinished,
            )
            previous = process
        }
        readings.forEach { (genesis, reading) ->
            val listed = reading.protocolBytes.entries
                .filter { (_, bytes) -> bytes.received + bytes.sent > 0 }
                .sortedByDescending { (_, bytes) -> bytes.received + bytes.sent }
                .joinToString(" ") { (protocol, bytes) ->
                    "%s=%.2f/%.2f".format(Locale.ROOT, protocol, mb(bytes.received), mb(bytes.sent))
                }
            Timber.tag(LOG_TAG).i("%s MB in/out: %s", genesis.take(8), listed)
        }
    }

    private fun mb(bytes: Double) = bytes / 1e6
}
