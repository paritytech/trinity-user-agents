package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import io.novasama.substrate_sdk_android.wsrpc.state.pendingRequests
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ConnectionPool
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.utils.combine
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_connection_status_api.domain.ChainHealthMonitor
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_impl.data.ChainHeadDataSource
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe.ChainHealthProbe
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe.ChainMetricContext
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring.ChainHealthThresholds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.shareIn
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * Builds a per-chain [ChainHealth] from the smoothed socket state and the readings of the pluggable
 * probe set. The pipeline is foreground-gated by the mixin and holds the chain socket up via the ref
 * counter for as long as it runs.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@Singleton
class RealChainHealthMonitor @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val connectionPool: ConnectionPool,
    private val chainStateRepository: ChainStateRepository,
    private val chainHeadDataSource: ChainHeadDataSource,
    private val connectionRefCounter: ChainConnectionRefCounter,
    private val connectionSmoother: ConnectionSmoother,
    private val networkStateService: NetworkStateService,
    private val probes: Set<@JvmSuppressWildcards ChainHealthProbe>,
) : ChainHealthMonitor {
    override fun observeChainsHealth(): Flow<List<ChainHealth>> =
        monitoredChainIds().map(::observeChainHealth).combine()

    private fun monitoredChainIds(): List<ChainId> = listOf(
        knownChains.people,
        knownChains.assetHub,
        knownChains.bulletIn,
    )

    private fun observeChainHealth(chainId: ChainId): Flow<ChainHealth> = channelFlow {
        connectionRefCounter.withConnectionEnabled(chainId, CONNECTION_LABEL) {
            val chain = chainRegistry.getChain(chainId)
            val blockTime = resolveBlockTime(chainId)

            val bestBlock = chainHeadDataSource.bestBlockNumber(chainId)
                .shareIn(this@channelFlow, SharingStarted.WhileSubscribed(), replay = 1)
            val pendingRequests = observePendingRequests(chainId)
                .shareIn(this@channelFlow, SharingStarted.WhileSubscribed(), replay = 1)
            val ticks = sharedSampleTicks(this@channelFlow, ChainHealthThresholds.SAMPLE_TICK)

            val connection = connectionSmoother.smooth(observeSocketState(chainId))
                .shareIn(this@channelFlow, SharingStarted.WhileSubscribed(), replay = 1)

            val context = ChainMetricContext(
                chainId = chainId,
                bestBlockNumber = bestBlock,
                expectedBlockTime = blockTime,
                pendingRequests = pendingRequests,
                connection = connection,
                ticks = ticks,
            )
            val readings = probes.map { it.observe(context) }.combine()

            combine(connection, readings) { presentation, readingList ->
                ChainHealth(
                    chainId = chainId,
                    chainName = chain.name,
                    connection = presentation,
                    expectedBlockTime = blockTime,
                    readings = readingList,
                )
            }
                .onEach { chainHealthLog.d("%s is %s with %s", it.chainName, it.connection, it.readings) }
                .collect { send(it) }
        }
    }

    private fun observeSocketState(chainId: ChainId): Flow<RawConnectivity> {
        val socketStates = chainRegistry.chainsById
            .map { connectionPool.getConnectionOrNull(chainId) }
            .distinctUntilChanged()
            .flatMapLatest { connection -> connection?.state ?: flowOf(null) }

        return rawConnectivity(socketStates, networkStateService.isNetworkAvailable)
    }

    private fun observePendingRequests(chainId: ChainId): Flow<Set<Any>> =
        chainRegistry.chainsById
            .map { connectionPool.getConnectionOrNull(chainId) }
            .distinctUntilChanged()
            .flatMapLatest { connection -> connection?.state ?: flowOf(null) }
            .map { state -> state?.pendingRequests.orEmpty() }

    private suspend fun resolveBlockTime(chainId: ChainId): Duration =
        runCancellableCatching { chainStateRepository.expectedBlockTime(chainId) }
            .onFailure { chainHealthLog.w(it, "%s block time unavailable, measuring against %s", chainId, FALLBACK_BLOCK_TIME) }
            .getOrDefault(FALLBACK_BLOCK_TIME)

    private companion object {
        const val CONNECTION_LABEL = "chain-health"
        val FALLBACK_BLOCK_TIME: Duration = 6.seconds
    }
}
