package io.paritytech.polkadotapp.chains.multiNetwork

import com.google.gson.Gson
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.paritytech.polkadotapp.chains.multiNetwork.chain.ChainSyncService
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapChainLocalToChain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapConnectionStateToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.ConnectionState
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnection
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ConnectionPool
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeProvider
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeProviderPool
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeSubscriptionPool
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeSyncService
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.types.BaseTypeSynchronizer
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.isFullSync
import io.paritytech.polkadotapp.chains.util.level
import io.paritytech.polkadotapp.chains.util.requiresBaseTypes
import io.paritytech.polkadotapp.chains.util.typesUsage
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.common.utils.removeHexPrefix
import io.paritytech.polkadotapp.database.dao.ChainDao
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.collections.first

data class ChainWithAsset(
    val chain: Chain,
    val asset: Chain.Asset,
)

@Singleton
class ChainRegistry @Inject constructor(
    private val runtimeProviderPool: RuntimeProviderPool,
    private val connectionPool: ConnectionPool,
    private val runtimeSubscriptionPool: RuntimeSubscriptionPool,
    private val chainDao: ChainDao,
    private val chainSyncService: ChainSyncService,
    private val baseTypeSynchronizer: BaseTypeSynchronizer,
    private val runtimeSyncService: RuntimeSyncService,
    private val gson: Gson,
    val knownChains: KnownChains
) : CoroutineScope by CoroutineScope(Dispatchers.Default) {
    val currentChains: Flow<List<Chain>> = chainDao.joinChainInfoFlow()
        .mapList { mapChainLocalToChain(it, gson) }
        .diffed()
        .map { diff ->
            diff.removed.forEach { unregisterChain(it) }
            diff.newOrUpdated.forEach { chain -> registerChain(chain) }

            diff.all
        }
        .filter { it.isNotEmpty() }
        .distinctUntilChanged()
        .inBackground()
        .shareIn(this, SharingStarted.Eagerly, replay = 1)

    val chainsById: Flow<Map<ChainId, Chain>> =
        currentChains.map { chains -> chains.associateBy { it.id } }
            .inBackground()
            .shareIn(this, SharingStarted.Eagerly, replay = 1)

    init {
        syncChainsAndAssets()

        syncBaseTypesIfNeeded()
    }

    suspend fun getChainIdByGenesisHash(genesisHash: GenesisHash): ChainId? {
        return currentChains.first().find { it.genesisHash == genesisHash }
            ?.id
    }

    suspend fun getConnection(chainId: ChainId): ChainConnection {
        requireConnectionStateAtLeast(chainId, ConnectionState.LIGHT_SYNC)

        return connectionPool.getConnection(chainId.removeHexPrefix())
    }

    suspend fun getConnectionOrNull(chainId: ChainId): ChainConnection? {
        requireConnectionStateAtLeast(chainId, ConnectionState.LIGHT_SYNC)

        return connectionPool.getConnectionOrNull(chainId.removeHexPrefix())
    }

    suspend fun getRuntimeProvider(chainId: ChainId): RuntimeProvider {
        requireConnectionStateAtLeast(chainId, ConnectionState.FULL_SYNC)

        return runtimeProviderPool.getRuntimeProvider(chainId.removeHexPrefix())
    }

    suspend fun getChain(chainId: ChainId): Chain {
        return chainsById.first().getValue(chainId.removeHexPrefix())
    }

    suspend fun peopleChain() = getChain(knownChains.people)

    suspend fun assetHub() = getChain(knownChains.assetHub)

    suspend fun bulletInChain() = getChain(knownChains.bulletIn)

    private suspend fun requireConnectionStateAtLeast(
        chainId: ChainId,
        state: ConnectionState,
    ) {
        val chain = getChain(chainId)

        if (chain.connectionState.level >= state.level) return

        Timber.d("Requested state $state for ${chain.name}, current is ${chain.connectionState}. Triggering state change to $state")

        chainDao.setConnectionState(chainId, mapConnectionStateToLocal(state))
        awaitConnectionStateIsAtLeast(chainId, state)
    }

    private fun syncChainsAndAssets() {
        launch {
            runCatching {
                chainSyncService.syncUp()
            }.onFailure {
                Timber.e(it, "Failed to sync chains or assets")
            }
        }
    }

    private suspend fun awaitConnectionStateIsAtLeast(
        chainId: ChainId,
        state: ConnectionState,
    ) {
        chainsById
            .mapNotNull { chainsById -> chainsById[chainId] }
            .first { it.connectionState.level >= state.level }
    }

    private fun unregisterChain(chain: Chain) {
        unregisterSubstrateServices(chain)
        unregisterConnections(chain.id)
    }

    private suspend fun registerChain(chain: Chain) {
        return when (chain.connectionState) {
            ConnectionState.FULL_SYNC -> registerFullSyncChain(chain)
            ConnectionState.LIGHT_SYNC -> registerLightSyncChain(chain)
            ConnectionState.DISABLED -> registerDisabledChain(chain)
        }
    }

    private fun registerDisabledChain(chain: Chain) {
        unregisterSubstrateServices(chain)
        unregisterConnections(chain.id)
    }

    private suspend fun registerLightSyncChain(chain: Chain) {
        registerConnection(chain)

        unregisterSubstrateServices(chain)
    }

    private suspend fun registerFullSyncChain(chain: Chain) {
        val connection = registerConnection(chain)

        if (chain.hasSubstrateRuntime) {
            runtimeProviderPool.setupRuntimeProvider(chain)
            runtimeSyncService.registerChain(chain, connection)
            runtimeSubscriptionPool.setupRuntimeSubscription(chain, connection)
        }
    }

    private suspend fun registerConnection(chain: Chain): ChainConnection {
        return connectionPool.setupConnection(chain)
    }

    private fun syncBaseTypesIfNeeded() =
        launch {
            val chains = currentChains.first()
            val needToSyncBaseTypes =
                chains.any { it.typesUsage.requiresBaseTypes && it.connectionState.shouldSyncRuntime() }

            if (needToSyncBaseTypes) {
                baseTypeSynchronizer.sync()
            }
        }

    private fun unregisterSubstrateServices(chain: Chain) {
        if (chain.hasSubstrateRuntime) {
            runtimeProviderPool.removeRuntimeProvider(chain.id)
            runtimeSubscriptionPool.removeSubscription(chain.id)
            runtimeSyncService.unregisterChain(chain.id)
        }
    }

    private fun unregisterConnections(chainId: ChainId) {
        connectionPool.removeConnection(chainId)
    }

    private fun ConnectionState.shouldSyncRuntime(): Boolean {
        return isFullSync
    }
}

suspend fun ChainRegistry.getChainIdByGenesisHashOrThrow(genesisHash: GenesisHash): ChainId {
    return requireNotNull(getChainIdByGenesisHash(genesisHash)) {
        "No chain found for genesis hash $genesisHash"
    }
}

suspend fun ChainRegistry.getChainOrNull(chainId: ChainId): Chain? {
    return chainsById.first()[chainId.removeHexPrefix()]
}

suspend fun ChainRegistry.awaitChain(chainId: ChainId): Chain {
    return chainsById
        .mapNotNull { it[chainId.removeHexPrefix()] }
        .first()
}

suspend fun ChainRegistry.getChain(genesisHash: GenesisHash): Chain {
    return currentChains.first().first { it.genesisHash == genesisHash }
}

suspend fun ChainRegistry.getChainOrNull(genesisHash: GenesisHash): Chain? {
    return currentChains.first().find { it.genesisHash == genesisHash }
}

suspend fun ChainRegistry.chainWithAssetOrNull(fullChainAssetId: FullChainAssetId): ChainWithAsset? {
    val chain = getChainOrNull(fullChainAssetId.chainId) ?: return null
    val chainAsset = chain.assetsById[fullChainAssetId.assetId] ?: return null

    return ChainWithAsset(chain, chainAsset)
}

suspend fun ChainRegistry.assetOrNull(fullChainAssetId: FullChainAssetId): Chain.Asset? {
    val chain = getChainOrNull(fullChainAssetId.chainId) ?: return null

    return chain.assetsById[fullChainAssetId.assetId]
}

suspend fun ChainRegistry.allAssets(): List<Chain.Asset> =
    currentChains.first().flatMap { it.assets }

suspend fun ChainRegistry.chainWithAsset(
    chainId: ChainId,
    assetId: Int,
): ChainWithAsset {
    val chain = chainsById.first().getValue(chainId)

    return ChainWithAsset(chain, chain.assetsById.getValue(assetId))
}

suspend fun ChainRegistry.chainWithAsset(fullChainAssetId: FullChainAssetId): ChainWithAsset {
    return chainWithAsset(fullChainAssetId.chainId, fullChainAssetId.assetId)
}

suspend fun ChainRegistry.asset(
    chainId: ChainId,
    assetId: Int,
): Chain.Asset {
    val chain = chainsById.first().getValue(chainId)

    return chain.assetsById.getValue(assetId)
}

suspend fun ChainRegistry.asset(fullChainAssetId: FullChainAssetId): Chain.Asset {
    return asset(fullChainAssetId.chainId, fullChainAssetId.assetId)
}

fun ChainsById.assets(ids: Collection<FullChainAssetId>): List<Chain.Asset> {
    return ids.map { (chainId, assetId) ->
        getValue(chainId).assetsById.getValue(assetId)
    }
}

suspend fun ChainRegistry.findRelayChainOrThrow(chainId: ChainId): ChainId {
    val chain = getChain(chainId)
    return chain.parentId ?: chainId
}

suspend inline fun ChainRegistry.findChain(predicate: (Chain) -> Boolean): Chain? =
    currentChains.first().firstOrNull(predicate)

suspend inline fun ChainRegistry.findChains(predicate: (Chain) -> Boolean): List<Chain> =
    currentChains.first().filter(predicate)

suspend inline fun ChainRegistry.findChainsById(predicate: (Chain) -> Boolean): ChainsById {
    return chainsById().filterValues { chain -> predicate(chain) }.asChainsById()
}

suspend fun ChainRegistry.getRuntime(chainId: ChainId) = getRuntimeProvider(chainId).get()

suspend fun ChainRegistry.getSocket(chainId: ChainId): SocketService =
    getConnection(chainId).socketService

suspend fun ChainRegistry.getSocketOrNull(chainId: ChainId): SocketService? =
    getConnectionOrNull(chainId)?.socketService

suspend fun ChainRegistry.chainsById(): ChainsById = ChainsById(chainsById.first())

suspend inline fun <R> ChainRegistry.withRuntime(chainId: ChainId, action: WithRuntime.() -> R): R {
    return with(InlineWithRuntime(getRuntime(chainId))) {
        action()
    }
}

inline fun <R> RuntimeSnapshot.provideContext(action: WithRuntime.() -> R): R {
    return with(InlineWithRuntime(this)) {
        action()
    }
}

@JvmInline
value class InlineWithRuntime(override val runtime: RuntimeSnapshot) : WithRuntime

suspend fun ChainRegistry.findEvmChain(evmChainId: Int): Chain? {
    return findChain { it.isEthereumBased && it.addressPrefix == evmChainId }
}

suspend fun ChainRegistry.findEvmChainFromHexId(evmChainIdHex: String): Chain? {
    val addressPrefix = evmChainIdHex.removeHexPrefix().toIntOrNull(radix = 16) ?: return null

    return findEvmChain(addressPrefix)
}
