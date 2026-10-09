package io.paritytech.polkadotapp.tools_remoteconfig_impl.domain

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.awaitTrue
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.logSuccess
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_impl.data.RemoteConfigDataSource
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject

class RealRemoteConfigService @Inject constructor(
    private val dataSource: RemoteConfigDataSource,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val gson: Gson,
) : RemoteConfigService {
    private val isSynced = MutableStateFlow(false)
    private val syncingMutex = Mutex()

    override fun init() {
        runCatching {
            dataSource.init()
        }
    }

    override suspend fun sync(): Result<Unit> = withContext(coroutineDispatchers.io) {
        syncingMutex.withLock {
            // Activated values stay valid once fetched; re-syncing would only reopen the window in which every
            // getSynced* caller is blocked, and a cancelled or failed re-sync would keep them blocked for good.
            if (isSynced.value) return@withLock Result.success(Unit)

            dataSource.sync()
                .onSuccess { isSynced.value = true }
                .logSuccess("Remote config synced")
                .logFailure("Failed to sync remote config")
        }
    }

    override suspend fun getString(key: String): Result<String> {
        return runCatching { dataSource.getString(key) }
    }

    override suspend fun getSyncedString(key: String): Result<String> {
        return getSyncedValue { dataSource.getString(key) }
    }

    override suspend fun getBoolean(key: String): Result<Boolean> {
        return runCatching { dataSource.getBoolean(key) }
    }

    override suspend fun getSyncedBoolean(key: String): Result<Boolean> {
        return getSyncedValue { dataSource.getBoolean(key) }
    }

    override suspend fun getLong(key: String): Result<Long> {
        return runCatching { dataSource.getLong(key) }
    }

    override suspend fun getSyncedLong(key: String): Result<Long> {
        return getSyncedValue { dataSource.getLong(key) }
    }

    override fun observeString(key: String): Flow<String> = flow {
        awaitSynced()
        emit(dataSource.getString(key))

        val updatesFlow = dataSource.configUpdates()
            .filter { updatedKeys -> key in updatedKeys }
            .map { dataSource.getString(key) }
        emitAll(updatesFlow)
    }.distinctUntilChanged()

    override suspend fun <T> getSyncedJsonObject(key: String, clazz: Class<T>): Result<T> {
        return getSyncedValue {
            val raw = dataSource.getString(key)
            gson.fromJson<T>(raw, clazz)
        }
    }

    private suspend inline fun <R> getSyncedValue(retrieve: () -> R): Result<R> {
        return runCatching {
            awaitSynced()
            retrieve()
        }
    }

    private suspend fun awaitSynced() {
        isSynced.awaitTrue()
    }
}
