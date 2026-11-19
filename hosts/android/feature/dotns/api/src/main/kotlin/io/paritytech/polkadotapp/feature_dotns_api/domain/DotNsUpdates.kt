package io.paritytech.polkadotapp.feature_dotns_api.domain

interface DotNsUpdates {
    suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Unit>

    fun namesOnDevice(): Set<String>
}
