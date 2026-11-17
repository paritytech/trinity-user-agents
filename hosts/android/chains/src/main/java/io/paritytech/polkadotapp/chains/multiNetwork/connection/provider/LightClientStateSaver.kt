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
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

private val FIRST_SAVE_DELAY = 30.seconds
private val SAVE_INTERVAL = 1.minutes
private val SAVE_INTERVAL_MAX = 10.minutes

/**
 * Stores the light client's finalized state while the app is in the foreground: first after
 * [FIRST_SAVE_DELAY], then at an interval doubling up to [SAVE_INTERVAL_MAX], since a snapshot costs
 * a round trip and a multi-megabyte write. Going to the background closes every connection, which
 * stops the chains, so there is nothing left to save then. With the light client off, it removes
 * what an earlier run stored instead.
 */
class LightClientStateSaver @Inject constructor(
    private val lightClient: LightClient,
    private val storage: LightClientStorage,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val dispatchers: CoroutineDispatchers,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        scope.launch(dispatchers.computation) {
            if (lightClient.servesAny()) {
                appLifecycleObserver.subscribeIsForeground().collectLatest { foreground ->
                    if (foreground) saveWhileForeground()
                }
            } else {
                storage.clear()
            }
        }
    }

    private suspend fun saveWhileForeground() {
        delay(FIRST_SAVE_DELAY)
        var interval: Duration = SAVE_INTERVAL
        while (true) {
            lightClient.saveDatabases()
            delay(interval)
            interval = (interval * 2).coerceAtMost(SAVE_INTERVAL_MAX)
        }
    }
}
