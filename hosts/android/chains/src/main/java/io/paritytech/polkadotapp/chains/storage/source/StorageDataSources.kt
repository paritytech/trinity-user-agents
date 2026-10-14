package io.paritytech.polkadotapp.chains.storage.source

import io.paritytech.polkadotapp.chains.di.LocalSourceQualifier
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class StorageDataSources @Inject constructor(
    @param:RemoteSourceQualifier val remote: StorageDataSource,
    @param:LocalSourceQualifier val local: StorageDataSource,
)

fun StorageDataSources.pickForDataConsistencyRequirement(
    consistency: CacheableDataConsistency,
): StorageDataSource = when (consistency) {
    CacheableDataConsistency.CONSISTENT_WITH_REMOTE -> remote
    CacheableDataConsistency.CAN_BE_STALE -> local
}
