package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import kotlinx.coroutines.flow.Flow
import kotlin.time.Duration

/**
 * Shared per-chain inputs handed to every probe. Every flow here is shared upstream, so probes
 * reading the same one do not open duplicate subscriptions or run duplicate timers.
 */
data class ChainMetricContext(
    val chainId: ChainId,
    val bestBlockNumber: Flow<Int>,
    val expectedBlockTime: Duration,
    // Requests currently pending on the socket, as stable identities (Sendable has no id, so tracked
    // by referential identity).
    val pendingRequests: Flow<Set<Any>>,
    val connection: Flow<ChainConnectionPresentation>,
    val ticks: Flow<Unit>,
)

/**
 * The extensibility seam: one probe per health metric. Binding a new probe with `@Binds @IntoSet` puts
 * its reading on the chain's health, but nothing displays readings directly — a metric reaches the UI only
 * through the presentation mapper, so a new one that should show up needs a branch there.
 */
interface ChainHealthProbe {
    fun observe(context: ChainMetricContext): Flow<ChainMetricReading>
}
