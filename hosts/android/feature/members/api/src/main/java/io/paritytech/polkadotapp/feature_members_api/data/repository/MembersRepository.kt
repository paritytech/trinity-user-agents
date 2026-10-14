package io.paritytech.polkadotapp.feature_members_api.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

interface MembersRepository {
    fun subscribeMembers(
        chainId: ChainId,
        keys: List<Pair<RingCollectionId, BandersnatchPublicKey>>,
        consistency: CacheableDataConsistency,
    ): Flow<Result<Map<Pair<RingCollectionId, BandersnatchPublicKey>, RingPosition?>>>

    suspend fun fetchMembers(
        chainId: ChainId,
        keys: List<Pair<RingCollectionId, BandersnatchPublicKey>>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash? = null,
    ): Result<Map<Pair<RingCollectionId, BandersnatchPublicKey>, RingPosition?>>

    suspend fun getRingKeys(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash? = null,
    ): Result<RingKeys>

    suspend fun getCollection(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<RingCollection>

    suspend fun getCollections(
        chainId: ChainId,
        collectionIds: List<RingCollectionId>,
        consistency: CacheableDataConsistency,
    ): Result<Map<RingCollectionId, RingCollection>>

    suspend fun getOnboardingSize(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<Int?>

    /** How many keys one page of a ring holds; a member's ring position counts within its page. */
    suspend fun getRingKeysPageSize(chainId: ChainId): Result<Int>

    suspend fun getRingsState(
        chainId: ChainId,
        collectionId: RingCollectionId,
        consistency: CacheableDataConsistency,
    ): Result<RingMembersState?>

    suspend fun getRingRoots(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash? = null,
    ): Result<Map<RingCollectionIdWithIndex, RingRoot>>

    suspend fun getRingStatuses(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash? = null,
    ): Result<Map<RingCollectionIdWithIndex, RingStatus>>

    fun subscribeRingStatuses(
        chainId: ChainId,
        keys: List<RingCollectionIdWithIndex>,
    ): Flow<Result<Map<RingCollectionIdWithIndex, RingStatus?>>>
}

fun MembersRepository.subscribeMembersInCollection(
    chainId: ChainId,
    collectionId: RingCollectionId,
    keys: List<BandersnatchPublicKey>,
    consistency: CacheableDataConsistency,
): Flow<Result<Map<BandersnatchPublicKey, RingPosition?>>> {
    return subscribeMembers(chainId, keys.map { collectionId to it }, consistency)
        .map { result -> result.map { it.mapKeys { (pair, _) -> pair.second } } }
}

suspend fun MembersRepository.fetchMembersInCollection(
    chainId: ChainId,
    collectionId: RingCollectionId,
    keys: List<BandersnatchPublicKey>,
    consistency: CacheableDataConsistency,
): Result<Map<BandersnatchPublicKey, RingPosition?>> {
    return fetchMembers(chainId, keys.map { collectionId to it }, consistency)
        .map { it.mapKeys { (pair, _) -> pair.second } }
}

fun MembersRepository.subscribeMember(
    chainId: ChainId,
    collectionId: RingCollectionId,
    key: BandersnatchPublicKey,
    consistency: CacheableDataConsistency,
): Flow<Result<RingPosition?>> {
    return subscribeMembers(chainId, listOf(collectionId to key), consistency)
        .map { result -> result.map { it[collectionId to key] } }
}

suspend fun MembersRepository.getMember(
    chainId: ChainId,
    collectionId: RingCollectionId,
    key: BandersnatchPublicKey,
    consistency: CacheableDataConsistency,
    blockHash: BlockHash? = null,
): Result<RingPosition?> {
    return fetchMembers(chainId, listOf(collectionId to key), consistency, blockHash)
        .map { it[collectionId to key] }
}

suspend fun MembersRepository.getRingRoot(
    chainId: ChainId,
    collectionId: RingCollectionId,
    ringIndex: RingIndex,
    consistency: CacheableDataConsistency,
    blockHash: BlockHash? = null,
): Result<RingRoot?> {
    return getRingRoots(chainId, listOf(collectionId to ringIndex), consistency, blockHash)
        .map { it[collectionId to ringIndex] }
}

suspend fun MembersRepository.getRingStatus(
    chainId: ChainId,
    collectionId: RingCollectionId,
    ringIndex: RingIndex,
    consistency: CacheableDataConsistency,
    blockHash: BlockHash? = null,
): Result<RingStatus?> {
    return getRingStatuses(chainId, listOf(collectionId to ringIndex), consistency, blockHash)
        .map { it[collectionId to ringIndex] }
}

fun MembersRepository.subscribeRingStatus(
    chainId: ChainId,
    collectionId: RingCollectionId,
    ringIndex: RingIndex,
): Flow<Result<RingStatus?>> {
    return subscribeRingStatuses(chainId, listOf(collectionId to ringIndex))
        .map { result -> result.map { it[collectionId to ringIndex] } }
}
