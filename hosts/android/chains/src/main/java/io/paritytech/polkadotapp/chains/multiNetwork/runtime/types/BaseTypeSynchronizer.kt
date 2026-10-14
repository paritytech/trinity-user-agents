package io.paritytech.polkadotapp.chains.multiNetwork.runtime.types

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.FileHash
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeFilesCache
import io.paritytech.polkadotapp.common.utils.md5
import io.paritytech.polkadotapp.common.utils.retryUntilDone
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class BaseTypeSynchronizer @Inject constructor(
    private val runtimeFilesCache: RuntimeFilesCache,
    private val typesFetcher: TypesFetcher,
) : CoroutineScope by CoroutineScope(Dispatchers.Default) {
    @Volatile
    private var syncJob: Job? = null

    private val _syncStatusFlow = MutableSharedFlow<FileHash>()
    val syncStatusFlow: Flow<FileHash> = _syncStatusFlow

    @Synchronized
    fun sync() {
        syncJob?.cancel()

        syncJob = launch {
            retryUntilDone {
                val definitions = typesFetcher.getBaseTypes()

                runtimeFilesCache.saveBaseTypes(definitions)

                _syncStatusFlow.emit(definitions.md5())

                syncJob = null
            }
        }
    }

    @Synchronized
    fun cacheNotFound() {
        if (syncJob == null) {
            sync()
        }
    }
}
