package io.paritytech.polkadotapp.feature_members_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSources
import io.paritytech.polkadotapp.chains.storage.source.pickForDataConsistencyRequirement
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.storage.source.subscribeCatching
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollection
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionIdWithIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingKeys
import io.paritytech.polkadotapp.feature_members_api.data.model.RingMembersState
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRoot
import io.paritytech.polkadotapp.feature_members_api.data.model.RingStatus
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.collections
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.maxFlexibleRingExponent
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.members
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.onboardingSize
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.ringKeys
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.ringKeysStatus
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.ringsState
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.root
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealMembersRepository @Inject constructor(
    private val storageDataSources: StorageDataSources,
) : MembersRepository {
    override fun subscribeMembers(
        chainId: ChainId,
        keys: List<Pair<RingCollectionId, BandersnatchPublicKey>>,
        consistency: CacheableDataConsistency,
    ): Flow<Result<Map<Pair<RingCollectionId, BandersnatchPublicKey>, RingPosition?>>> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).subscribeCatching(chainId) {
            metadata.members.members.observe(keys)
        }
    }

    override suspend fun fetchMembers(
        chainId: ChainId,
        keys: List<Pair<RingCollectionId, BandersnatchPublicKey>>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash?,
    ): Result<Map<Pair<RingCollectionId, BandersnatchPublicKey>, RingPosition?>> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId, blockHash) {
            metadata.members.members.entries(keys)
        }
    }

    override suspend fun getRingKeys(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash?,
    ): Result<RingKeys> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId, blockHash) {
            metadata.members.ringKeys.entries(collectionId, ringIndex)
                .entries
                .sortedBy { it.key.third }
                .flatMap { it.value.orEmpty() }
        }
    }

    override suspend fun getCollection(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<RingCollection> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId) {
            metadata.members.collections.queryNonNull(collectionId)
        }
    }

    override suspend fun getCollections(
        chainId: ChainId,
        collectionIds: List<RingCollectionId>,
        consistency: CacheableDataConsistency,
    ): Result<Map<RingCollectionId, RingCollection>> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId) {
            metadata.members.collections.entries(collectionIds)
        }
    }

    override suspend fun getOnboardingSize(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<Int?> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId) {
            metadata.members.onboardingSize.query(collectionId)
        }
    }

    override suspend fun getRingKeysPageSize(chainId: ChainId): Result<Int> {
        return storageDataSources.remote.queryCatching(chainId) {
            metadata.members.maxFlexibleRingExponent.ringCapacity
        }
    }

    override suspend fun getRingsState(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<RingMembersState?> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId) {
            metadata.members.ringsState.query(collectionId)
        }
    }

    override suspend fun getRingRoots(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash?,
    ): Result<Map<RingCollectionIdWithIndex, RingRoot>> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId, blockHash) {
            metadata.members.root.entries(keys)
        }
    }

    override suspend fun getRingStatuses(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash?,
    ): Result<Map<RingCollectionIdWithIndex, RingStatus>> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId, blockHash) {
            metadata.members.ringKeysStatus.entries(keys)
        }
    }

    override fun subscribeRingStatuses(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
    ): Flow<Result<Map<RingCollectionIdWithIndex, RingStatus?>>> {
        return storageDataSources.remote.subscribeCatching(chainId) {
            metadata.members.ringKeysStatus.observe(keys)
        }
    }
}
