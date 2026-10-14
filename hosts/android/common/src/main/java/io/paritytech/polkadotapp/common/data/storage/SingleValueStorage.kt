package io.paritytech.polkadotapp.common.data.storage

import kotlinx.coroutines.flow.Flow

interface SingleValueStorage<T> {
    fun valueFlow(): Flow<T?>

    suspend fun getValue(): T?

    suspend fun saveValue(value: T)

    suspend fun removeValue()

    suspend fun requireValue(): T
}
