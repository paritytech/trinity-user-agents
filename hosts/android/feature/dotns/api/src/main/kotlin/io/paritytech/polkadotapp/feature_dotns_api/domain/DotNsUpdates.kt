package io.paritytech.polkadotapp.feature_dotns_api.domain

import kotlinx.coroutines.flow.Flow

interface DotNsUpdates {
    suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Unit>

    suspend fun applyUpdates(dotNsNames: Collection<String>)

    fun observePendingUpdates(): Flow<Set<String>>

    fun namesOnDevice(): Set<String>
}
