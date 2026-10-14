package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.data.memory.getCatching
import io.paritytech.polkadotapp.common.utils.scale.BigEndianU32Scale
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.getLiteStmtStoreSlotsPerPeriod
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.getStmtStoreReplacementCooldown
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.getStmtStoreSlotsPerPeriod
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model.StmtStoreAllowanceEntry
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.statementStoreAllowances
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.statementStoreResources
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class RealStatementStoreSlotRepository @Inject constructor(
    @RemoteSourceQualifier private val storageDataSource: StorageDataSource,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
    private val knownChains: KnownChains,
) : StatementStoreSlotRepository {
    // The runtime exposes these as view functions, so they are read once per session rather than per call.
    // Unwrapping inside the compute keeps a failed read out of the cache; getCatching re-wraps the throw.
    private val slotsPerPeriodCache = SingleValueCache {
        resourcesViewFunctions().getStmtStoreSlotsPerPeriod().getOrThrow()
    }

    private val liteSlotsPerPeriodCache = SingleValueCache {
        resourcesViewFunctions().getLiteStmtStoreSlotsPerPeriod().getOrThrow()
    }

    private val replacementCooldownCache = SingleValueCache {
        resourcesViewFunctions().getStmtStoreReplacementCooldown().getOrThrow().toLong().seconds
    }

    override suspend fun maxSlotsPerPeriod(collection: PeopleCollection): Result<UInt> {
        return when (collection) {
            PeopleCollection.People -> slotsPerPeriodCache.getCatching()
            PeopleCollection.LitePeople -> liteSlotsPerPeriodCache.getCatching()
        }
    }

    override suspend fun replacementCooldown(): Result<Duration> {
        return replacementCooldownCache.getCatching()
    }

    override suspend fun allowanceEntries(
        chainId: ChainId,
        period: UInt,
        candidates: Collection<BandersnatchAlias>,
    ): Map<BandersnatchAlias, StmtStoreAllowanceEntry> {
        if (candidates.isEmpty()) return emptyMap()
        return storageDataSource.query(chainId) {
            val periodKey = BigEndianU32Scale(period)
            val keys = candidates.map { periodKey to it }
            runtime.metadata.statementStoreResources.statementStoreAllowances.entries(keys)
                .mapKeys { (compositeKey, _) -> compositeKey.second }
        }
    }

    private suspend fun resourcesViewFunctions() = viewFunctionsApi.forChain(knownChains.people)
}
