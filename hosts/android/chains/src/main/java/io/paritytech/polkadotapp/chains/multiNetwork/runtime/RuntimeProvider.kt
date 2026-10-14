package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.TypesUsage
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.types.BaseTypeSynchronizer
import io.paritytech.polkadotapp.chains.util.typesUsage
import io.paritytech.polkadotapp.common.utils.inFractionalSeconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import kotlin.time.ExperimentalTime
import kotlin.time.measureTimedValue

class RuntimeProvider(
    private val runtimeFactory: RuntimeFactory,
    private val runtimeSyncService: RuntimeSyncService,
    private val baseTypeSynchronizer: BaseTypeSynchronizer,
    chain: Chain,
) : CoroutineScope by CoroutineScope(Dispatchers.Default) {
    private val chainId = chain.id

    private var typesUsage = chain.typesUsage

    private val runtimeFlow = MutableSharedFlow<ConstructedRuntime>(replay = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    private var currentConstructionJob: Job? = null

    suspend fun get(): RuntimeSnapshot {
        val runtime = runtimeFlow.first()

        return runtime.runtime
    }

    fun observe(): Flow<RuntimeSnapshot> = runtimeFlow.map { it.runtime }

    init {
        baseTypeSynchronizer.syncStatusFlow
            .onEach(::considerReconstructingRuntime)
            .launchIn(this)

        runtimeSyncService.syncResultFlow(chainId)
            .onEach(::considerReconstructingRuntime)
            .launchIn(this)

        tryLoadFromCache()
    }

    fun finish() {
        invalidateRuntime()

        cancel()
    }

    fun considerUpdatingTypesUsage(newTypesUsage: TypesUsage) {
        if (typesUsage != newTypesUsage) {
            typesUsage = newTypesUsage

            constructNewRuntime(typesUsage)
        }
    }

    private fun tryLoadFromCache() {
        constructNewRuntime(typesUsage)
    }

    private fun considerReconstructingRuntime(runtimeSyncResult: SyncResult) {
        launch {
            currentConstructionJob?.join()

            val currentVersion = runtimeFlow.replayCache.firstOrNull()

            if (
                currentVersion == null ||
                // metadata was synced and new hash is different from current one
                (runtimeSyncResult.metadataHash != null && currentVersion.metadataHash != runtimeSyncResult.metadataHash) ||
                // types were synced and new hash is different from current one
                (runtimeSyncResult.typesHash != null && currentVersion.ownTypesHash != runtimeSyncResult.typesHash)
            ) {
                constructNewRuntime(typesUsage)
            }
        }
    }

    private fun considerReconstructingRuntime(newBaseTypesHash: String) {
        launch {
            currentConstructionJob?.join()

            val currentVersion = runtimeFlow.replayCache.firstOrNull()

            if (typesUsage == TypesUsage.OWN) {
                return@launch
            }

            if (
                currentVersion == null ||
                currentVersion.baseTypesHash != newBaseTypesHash
            ) {
                constructNewRuntime(typesUsage)
            }
        }
    }

    @OptIn(ExperimentalTime::class)
    private fun constructNewRuntime(typesUsage: TypesUsage) {
        currentConstructionJob?.cancel()

        currentConstructionJob =
            launch {
                invalidateRuntime()

                runCatching {
                    val (value, duration) = measureTimedValue { runtimeFactory.constructRuntime(chainId, typesUsage) }

                    Timber.d("Constructed runtime for $chainId in ${duration.inFractionalSeconds} seconds")

                    runtimeFlow.emit(value)
                }.onFailure {
                    when (it) {
                        ChainInfoNotInCacheException -> {
                            runtimeSyncService.cacheNotFound(chainId)

                            Timber.w("Runtime cache was not found for $chainId")
                        }

                        BaseTypesNotInCacheException -> {
                            baseTypeSynchronizer.cacheNotFound()

                            Timber.w("Base types cache were not found")
                        }

                        NoRuntimeVersionException -> {
                            Timber.w("Runtime version for $chainId was not found in database")
                        } // pass
                        else -> Timber.e(it, "Failed to construct runtime ($chainId)")
                    }
                }

                currentConstructionJob = null
            }
    }

    private fun invalidateRuntime() {
        runtimeFlow.resetReplayCache()
    }
}
