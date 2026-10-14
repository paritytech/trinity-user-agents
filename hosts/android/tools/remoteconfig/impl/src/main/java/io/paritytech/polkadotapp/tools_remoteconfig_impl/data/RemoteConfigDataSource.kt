package io.paritytech.polkadotapp.tools_remoteconfig_impl.data

import kotlinx.coroutines.flow.Flow

interface RemoteConfigDataSource {
    fun init()

    suspend fun sync(): Result<Unit>

    suspend fun getString(key: String): String

    suspend fun getBoolean(key: String): Boolean

    suspend fun getLong(key: String): Long

    fun configUpdates(): Flow<Set<String>>
}
