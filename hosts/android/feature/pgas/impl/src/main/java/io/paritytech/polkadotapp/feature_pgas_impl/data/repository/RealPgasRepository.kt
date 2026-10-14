package io.paritytech.polkadotapp.feature_pgas_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.scale.BigEndianU32Scale
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain.claimedGasAliases
import io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain.maxClaimsPerPeriodPerLitePerson
import io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain.maxClaimsPerPeriodPerPerson
import io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain.pgas
import io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain.pgasClaimAmount
import javax.inject.Inject

class RealPgasRepository @Inject constructor(
    @RemoteSourceQualifier private val storageDataSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
) : PgasRepository {
    override suspend fun maxClaimsPerPeriod(chainId: ChainId, collection: PeopleCollection): UInt {
        return chainRegistry.withRuntime(chainId) {
            when (collection) {
                PeopleCollection.People -> runtime.metadata.pgas.maxClaimsPerPeriodPerPerson
                PeopleCollection.LitePeople -> runtime.metadata.pgas.maxClaimsPerPeriodPerLitePerson
            }
        }
    }

    override suspend fun claimedAliases(
        chainId: ChainId,
        period: UInt,
        candidates: List<BandersnatchAlias>,
    ): Set<BandersnatchAlias> {
        if (candidates.isEmpty()) return emptySet()

        return storageDataSource.query(chainId) {
            val periodKey = BigEndianU32Scale(period)
            val keys = candidates.map { periodKey to it }
            runtime.metadata.pgas.claimedGasAliases.findExistingKeys(keys)
                .mapToSet { (_, alias) -> alias }
        }
    }

    override suspend fun currentClaimAmount(chainAsset: Chain.Asset): Balance {
        return chainRegistry.withRuntime(chainAsset.chainId) {
            runtime.metadata.pgas.pgasClaimAmount.intoBalance()
        }
    }
}
