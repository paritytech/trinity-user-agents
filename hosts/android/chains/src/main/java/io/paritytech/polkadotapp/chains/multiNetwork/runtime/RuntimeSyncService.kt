package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnection
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.types.TypesFetcher
import io.paritytech.polkadotapp.common.utils.md5
import io.paritytech.polkadotapp.common.utils.newLimitedThreadPoolExecutor
import io.paritytech.polkadotapp.common.utils.retryUntilDone
import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.database.model.chain.ChainRuntimeInfoLocal
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

data class SyncInfo(
    val connection: ChainConnection,
    val typesUrl: String?,
)

class SyncResult(
    val chainId: String,
    val metadataHash: FileHash?,
    val typesHash: FileHash?,
)

@Singleton
class RuntimeSyncService @Inject constructor(
    private val typesFetcher: TypesFetcher,
    private val runtimeFilesCache: RuntimeFilesCache,
    private val chainDao: ChainDao,
    private val runtimeMetadataFetcher: RuntimeMetadataFetcher,
    private val cacheMigrator: RuntimeCacheMigrator,
) : CoroutineScope by CoroutineScope(Dispatchers.Default) {
    companion object {
        private const val MAX_CONCURRENT_UPDATES: Int = 8
    }

    private val syncDispatcher = newLimitedThreadPoolExecutor(MAX_CONCURRENT_UPDATES)
        .asCoroutineDispatcher()
    private val knownChains = ConcurrentHashMap<String, SyncInfo>()

    private val syncingChains = ConcurrentHashMap<String, Job>()

    private val syncStatusFlow = MutableSharedFlow<SyncResult>()

    fun syncResultFlow(forChain: String): Flow<SyncResult> {
        return syncStatusFlow.filter { it.chainId == forChain }
    }

    fun applyRuntimeVersion(chainId: String) {
        launchSync(chainId)
    }

    fun registerChain(
        chain: Chain,
        connection: ChainConnection,
    ) {
        val existingSyncInfo = knownChains[chain.id]

        val newSyncInfo =
            SyncInfo(
                connection = connection,
                typesUrl = chain.types?.url
            )

        knownChains[chain.id] = newSyncInfo

        if (existingSyncInfo != null && existingSyncInfo != newSyncInfo) {
            launchSync(chain.id)
        }
    }

    fun unregisterChain(chainId: String) {
        knownChains.remove(chainId)

        cancelExistingSync(chainId)
    }

    // Android may clear cache files sometimes so it necessary to have force sync mechanism
    fun cacheNotFound(chainId: String) {
        if (!syncingChains.contains(chainId)) {
            launchSync(chainId, forceFullSync = true)
        }
    }

    fun isSyncing(chainId: String): Boolean {
        return syncingChains.containsKey(chainId)
    }

    private fun launchSync(
        chainId: String,
        forceFullSync: Boolean = false,
    ) {
        cancelExistingSync(chainId)

        syncingChains[chainId] =
            launch(syncDispatcher) {
                val syncResult =
                    runCatching {
                        sync(chainId, forceFullSync)
                    }.getOrNull()

                syncFinished(chainId)

                syncResult?.let { syncStatusFlow.emit(it) }
            }
    }

    private suspend fun sync(
        chainId: String,
        forceFullSync: Boolean,
    ): SyncResult? {
        val syncInfo = knownChains[chainId]

        if (syncInfo == null) {
            Timber.w("Unknown chain with id $chainId requested to be synced")
            return null
        }

        val runtimeInfo = chainDao.runtimeInfo(chainId) ?: return null

        val shouldSyncMetadata = runtimeInfo.shouldSyncMetadata() || forceFullSync

        val metadataHash = if (shouldSyncMetadata) {
            val runtimeMetadata = runtimeMetadataFetcher.fetchRawMetadata(syncInfo.connection.socketService)

            runtimeFilesCache.saveChainMetadata(chainId, runtimeMetadata)

            chainDao.updateSyncedRuntimeVersion(
                chainId = chainId,
                syncedVersion = runtimeInfo.remoteVersion,
                syncedTransactionVersion = runtimeInfo.transactionVersion,
                localMigratorVersion = cacheMigrator.latestVersion()
            )

            runtimeMetadata.md5()
        } else {
            null
        }

        val typesHash =
            syncInfo.typesUrl?.let { typesUrl ->
                retryUntilDone {
                    val types = typesFetcher.getTypes(typesUrl)

                    runtimeFilesCache.saveChainTypes(chainId, types)

                    types.md5()
                }
            }

        return SyncResult(
            metadataHash = metadataHash,
            typesHash = typesHash,
            chainId = chainId
        )
    }

    private fun cancelExistingSync(chainId: String) {
        syncingChains.remove(chainId)?.apply { cancel() }
    }

    private fun syncFinished(chainId: String) {
        syncingChains.remove(chainId)
    }

    private fun ChainRuntimeInfoLocal.shouldSyncMetadata() =
        syncedVersion != remoteVersion ||
            syncedTransactionVersion != transactionVersion ||
            cacheMigrator.needsMetadataFetch(localMigratorVersion)
}
