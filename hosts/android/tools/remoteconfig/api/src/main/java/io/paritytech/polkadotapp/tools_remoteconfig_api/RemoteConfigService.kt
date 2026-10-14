package io.paritytech.polkadotapp.tools_remoteconfig_api

import kotlinx.coroutines.flow.Flow

interface RemoteConfigService {
    /**
     * This method has to be called once
     */
    fun init()

    suspend fun sync(): Result<Unit>

    suspend fun getSyncedString(key: String): Result<String>

    suspend fun getString(key: String): Result<String>

    suspend fun getSyncedBoolean(key: String): Result<Boolean>

    suspend fun getBoolean(key: String): Result<Boolean>

    suspend fun getSyncedLong(key: String): Result<Long>

    suspend fun getLong(key: String): Result<Long>

    fun observeString(key: String): Flow<String>

    /**
     * Parses [T] from JSON value. [T] should not be a generic type
     */
    suspend fun <T> getSyncedJsonObject(key: String, clazz: Class<T>): Result<T>
}

suspend inline fun <reified T> RemoteConfigService.getSyncedJsonObject(key: String): Result<T> {
    return getSyncedJsonObject(key, T::class.java)
}
