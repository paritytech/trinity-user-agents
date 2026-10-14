package io.paritytech.polkadotapp.feature_members_api.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.source.query.WithRawValue
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCommitmentRecord
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import timber.log.Timber

interface MembersSubscriberRepository {
    suspend fun getRingRoots(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        consistency: CacheableDataConsistency,
        blockHash: BlockHash? = null,
    ): Result<List<RingCommitmentRecord>?>

    fun subscribeRingRoots(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
    ): Flow<Result<WithRawValue<List<RingCommitmentRecord>?>>>
}

suspend fun MembersSubscriberRepository.awaitRingRevision(
    chainId: ChainId,
    collectionId: RingCollectionId,
    ringIndex: RingIndex,
    revision: RingRevision,
): Result<Unit> = runCatching {
    Timber.i("awaitRingRevision: waiting for revision=$revision in collection=$collectionId ring=$ringIndex on chain=$chainId")

    subscribeRingRoots(chainId, collectionId, ringIndex)
        .map { it.getOrThrow() }
        .first { withRaw ->
            val records = withRaw.value.orEmpty()
            val found = records.any { it.revision == revision }

            if (records.isNotEmpty()) {
                val lowestAvailable = records.minOf { it.revision }
                check(lowestAvailable <= revision) {
                    "Revision $revision is no longer available on subscriber " +
                        "(lowest in sliding window: $lowestAvailable)"
                }
            }

            if (found) {
                Timber.i("awaitRingRevision: revision=$revision found at block=${withRaw.at}")
            }

            found
        }
}
