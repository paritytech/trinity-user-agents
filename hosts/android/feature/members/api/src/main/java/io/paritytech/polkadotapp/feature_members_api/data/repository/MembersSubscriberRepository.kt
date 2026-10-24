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
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import kotlin.time.Duration

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

/** The subscriber cannot verify a proof against [revision]; rebuild it against the current ring. */
class RingRevisionUnavailableException(val revision: RingRevision, message: String) : IllegalStateException(message)

suspend fun MembersSubscriberRepository.awaitRingRevision(
    chainId: ChainId,
    collectionId: RingCollectionId,
    ringIndex: RingIndex,
    revision: RingRevision,
    wait: Duration = Duration.INFINITE,
): Result<Unit> = runCatching {
    Timber.i("awaitRingRevision: waiting for revision=$revision in collection=$collectionId ring=$ringIndex on chain=$chainId")

    withTimeoutOrNull(wait) {
        subscribeRingRoots(chainId, collectionId, ringIndex)
            .map { it.getOrThrow() }
            .first { withRaw ->
                val held = withRaw.value.orEmpty().map { it.revision }
                when (ringRevisionStatus(held, revision)) {
                    RingRevisionStatus.IMPORTED -> {
                        Timber.i("awaitRingRevision: revision=$revision found at block=${withRaw.at}")
                        true
                    }

                    RingRevisionStatus.PRUNED -> throw RingRevisionUnavailableException(
                        revision,
                        "Revision $revision is no longer available on subscriber (held: $held)"
                    )

                    RingRevisionStatus.PENDING -> false
                }
            }
    } ?: throw RingRevisionUnavailableException(revision, "Revision $revision not imported on subscriber within $wait")
}

enum class RingRevisionStatus { IMPORTED, PRUNED, PENDING }

/**
 * The newest held root decides, not the oldest: the window drops revisions off the front but can also skip one,
 * and a skipped revision is as unreachable as an evicted one.
 */
fun ringRevisionStatus(held: List<RingRevision>, revision: RingRevision): RingRevisionStatus = when {
    revision in held -> RingRevisionStatus.IMPORTED
    held.any { it > revision } -> RingRevisionStatus.PRUNED
    else -> RingRevisionStatus.PENDING
}
