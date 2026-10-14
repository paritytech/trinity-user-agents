package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.data.memory.getCatching
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.scale.BigEndianU32Scale
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.getLongTermStorageClaimsPerPeriod
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.longTermStoragePeriodDuration
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.ltsResources
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.spentLongTermStorageAliases
import javax.inject.Inject

class RealLongTermStorageSlotRepository @Inject constructor(
    @RemoteSourceQualifier private val storageDataSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
    private val knownChains: KnownChains,
) : LongTermStorageSlotRepository {
    // The runtime exposes this as a view function, so it is read once per session rather than per call.
    // Unwrapping inside the compute keeps a failed read out of the cache; getCatching re-wraps the throw.
    private val maxClaimsPerPeriodCache = SingleValueCache {
        viewFunctionsApi.forChain(knownChains.people).getLongTermStorageClaimsPerPeriod().getOrThrow()
    }

    override suspend fun periodDurationSeconds(chainId: ChainId): UInt {
        return chainRegistry.withRuntime(chainId) {
            runtime.metadata.ltsResources.longTermStoragePeriodDuration
        }
    }

    override suspend fun maxClaimsPerPeriod(): Result<UByte> {
        return maxClaimsPerPeriodCache.getCatching()
    }

    override suspend fun spentAliases(
        chainId: ChainId,
        period: UInt,
        candidates: List<BandersnatchAlias>,
    ): Set<BandersnatchAlias> {
        if (candidates.isEmpty()) return emptySet()
        return storageDataSource.query(chainId) {
            val periodKey = BigEndianU32Scale(period)
            val keys = candidates.map { periodKey to it }
            runtime.metadata.ltsResources.spentLongTermStorageAliases.findExistingKeys(keys)
                .mapToSet { (_, alias) -> alias }
        }
    }
}
