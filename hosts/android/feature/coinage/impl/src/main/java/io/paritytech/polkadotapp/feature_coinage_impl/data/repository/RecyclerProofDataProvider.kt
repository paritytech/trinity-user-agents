package io.paritytech.polkadotapp.feature_coinage_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toStorageKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import javax.inject.Inject

interface RecyclerProofDataProvider {
    suspend fun getRecyclerRevisions(
        chainId: ChainId,
        recyclerKeys: Collection<RecyclerKey>,
        blockHash: BlockHash? = null
    ): Result<Map<RecyclerKey, RingRevision>>
}

class RealRecyclerProofDataProvider @Inject constructor(
    private val membersRepository: MembersRepository,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : RecyclerProofDataProvider {
    override suspend fun getRecyclerRevisions(
        chainId: ChainId,
        recyclerKeys: Collection<RecyclerKey>,
        blockHash: BlockHash?
    ): Result<Map<RecyclerKey, RingRevision>> {
        return coinageInstanceIdProvider.instanceId().flatMap { instanceId ->
            val storageKeys = recyclerKeys.map { it.toStorageKey(instanceId) }

            membersRepository.getRingRoots(
                chainId = chainId,
                keys = storageKeys,
                consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
                blockHash = blockHash
            )
                .map {
                    it
                        .mapKeys { (pair, _) -> pair.toRecyclerKey() }
                        .mapValues { (_, ringRoot) -> ringRoot.revision }
                }
        }
    }
}
