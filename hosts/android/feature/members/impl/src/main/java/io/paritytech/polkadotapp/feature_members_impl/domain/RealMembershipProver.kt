package io.paritytech.polkadotapp.feature_members_impl.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchDomainSize
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProofResult
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.bandersnatch_crypto.createProof
import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.repository.currentBlockHashCatching
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingKeys
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRoot
import io.paritytech.polkadotapp.feature_members_api.data.model.ringIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.toDomainSize
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.getMember
import io.paritytech.polkadotapp.feature_members_api.data.repository.getRingRoot
import io.paritytech.polkadotapp.feature_members_api.data.repository.getRingStatus
import io.paritytech.polkadotapp.feature_members_api.domain.MembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.PrecomputedMemberMembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.RingVrfMembershipProof
import io.paritytech.polkadotapp.feature_members_api.domain.RingVrfProofError
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import javax.inject.Inject

class RealMembershipProver @Inject constructor(
    private val membersRepository: MembersRepository,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val chainStateRepository: ChainStateRepository,
) : MembershipProver {
    override suspend fun proofMembership(
        member: MemberSource,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash?,
    ): Result<BandersnatchProofResult> {
        return precomputeRing(chainId, collectionId, ringIndex, blockHash)
            .flatMap { it.proofMembership(member, message, context) }
    }

    override suspend fun proofMembershipBatched(
        members: List<MemberSource>,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash?,
    ): Result<List<BandersnatchProof>> {
        return precomputeRing(chainId, collectionId, ringIndex, blockHash)
            .flatMap { it.proofMembershipBatched(members, message, context) }
    }

    override suspend fun createRingVrfProof(
        member: MemberSource,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        blockHash: BlockHash?,
    ): Result<RingVrfMembershipProof> {
        return precomputeForMember(member, chainId, collectionId, blockHash)
            .flatMap { it.createRingVrfProof(message, context) }
            .mapErrorNotInstance<_, RingVrfProofError> { RingVrfProofError.DataFetchFailed(it) }
    }

    override suspend fun precomputeForMember(
        member: MemberSource,
        chainId: ChainId,
        collectionId: RingCollectionId,
        blockHash: BlockHash?,
    ): Result<PrecomputedMemberMembershipProver> {
        val pinnedBlockHashResult = pinnedBlockHash(chainId, blockHash)
        val memberKeyResult = runCancellableCatching { member.toEntropy().memberKey() }

        return combineResults(pinnedBlockHashResult, memberKeyResult) { pinned, memberKey -> pinned to memberKey }
            .flatMap { (pinnedBlockHash, memberKey) ->
                resolveMemberRing(chainId, collectionId, memberKey, pinnedBlockHash).flatMap { (ringIndex, ringRoot) ->
                    precomputeRingPinned(chainId, collectionId, ringIndex, pinnedBlockHash).map { ring ->
                        PrecomputedMember(member, ringIndex, ringRoot.revision, ring)
                    }
                }
            }
            .mapErrorNotInstance<_, RingVrfProofError> { RingVrfProofError.DataFetchFailed(it) }
    }

    private suspend fun precomputeRing(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash?,
    ): Result<PrecomputedRing> = pinnedBlockHash(chainId, blockHash).flatMap { pinnedBlockHash ->
        precomputeRingPinned(chainId, collectionId, ringIndex, pinnedBlockHash)
    }

    private suspend fun precomputeRingPinned(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        pinnedBlockHash: BlockHash,
    ): Result<PrecomputedRing> = coroutineScope {
        val ringMembersDeferred = async { fetchIncludedRingMembers(chainId, collectionId, ringIndex, pinnedBlockHash) }
        val domainSizeDeferred = async { fetchDomainSize(chainId, collectionId) }

        combineResults(ringMembersDeferred.await(), domainSizeDeferred.await()) { ringMembers, domainSize ->
            PrecomputedRing(ringMembers, domainSize)
        }
    }

    /** One ring's members and domain size, resolved once and reused across proofs against it. */
    private inner class PrecomputedRing(
        private val ringMembers: RingKeys,
        private val domainSize: BandersnatchDomainSize,
    ) {
        suspend fun proofMembership(
            member: MemberSource,
            message: ByteArray,
            context: BandersnatchContext,
        ): Result<BandersnatchProofResult> = runCancellableCatching {
            member.toEntropy().createProof(ringMembers, message, context.value, domainSize)
        }

        suspend fun proofMembershipBatched(
            members: List<MemberSource>,
            message: ByteArray,
            context: BandersnatchContext,
        ): Result<List<BandersnatchProof>> = runCancellableCatching {
            members.map { member ->
                member.toEntropy().createProof(ringMembers, message, context.value, domainSize).proof
            }
        }
    }

    private class PrecomputedMember(
        private val member: MemberSource,
        private val ringIndex: RingIndex,
        private val ringRevision: RingRevision,
        private val ring: PrecomputedRing,
    ) : PrecomputedMemberMembershipProver {
        override suspend fun createRingVrfProof(
            message: ByteArray,
            context: BandersnatchContext,
        ): Result<RingVrfMembershipProof> {
            return ring.proofMembership(member, message, context)
                .map { RingVrfMembershipProof(it.proof, it.alias, ringIndex, ringRevision) }
        }
    }

    private suspend fun resolveMemberRing(
        chainId: ChainId,
        collectionId: RingCollectionId,
        memberKey: BandersnatchPublicKey,
        pinnedBlockHash: BlockHash,
    ): Result<Pair<RingIndex, RingRoot>> {
        return membersRepository.getMember(
            chainId = chainId,
            collectionId = collectionId,
            key = memberKey,
            consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
            blockHash = pinnedBlockHash,
        ).flatMap { position ->
            val ringIndex = position?.ringIndex
                ?: return@flatMap Result.failure(RingVrfProofError.NotMember)

            membersRepository.getRingRoot(
                chainId = chainId,
                collectionId = collectionId,
                ringIndex = ringIndex,
                consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
                blockHash = pinnedBlockHash,
            ).flatMap { ringRoot ->
                ringRoot?.let { Result.success(ringIndex to it) }
                    ?: Result.failure(RingVrfProofError.RingNotFound)
            }
        }
    }

    private suspend fun MemberSource.toEntropy(): BandersnatchEntropy {
        return when (this) {
            is MemberSource.Entropy -> bandersnatchEntropy
            is MemberSource.Account -> bandersnatchSecretsStorage.getEntropy(metaId)
        }
    }

    // Resolving the current block hash is a fallible RPC, so keep it inside the Result.
    private suspend fun pinnedBlockHash(chainId: ChainId, blockHash: BlockHash?): Result<BlockHash> {
        return blockHash?.let { Result.success(it) } ?: chainStateRepository.currentBlockHashCatching(chainId)
    }

    private suspend fun fetchIncludedRingMembers(
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash?,
    ): Result<RingKeys> = coroutineScope {
        val pageMembersDeferred = async {
            membersRepository.getRingKeys(
                chainId = chainId,
                collectionId = collectionId,
                ringIndex = ringIndex,
                consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
                blockHash = blockHash,
            )
        }
        val ringStatusDeferred = async {
            membersRepository.getRingStatus(
                chainId = chainId,
                collectionId = collectionId,
                ringIndex = ringIndex,
                consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
                blockHash = blockHash,
            ).requireNotNull()
        }

        combineResults(pageMembersDeferred.await(), ringStatusDeferred.await()) { members, status ->
            members.take(status.included)
        }
    }

    private suspend fun fetchDomainSize(
        chainId: ChainId,
        collectionId: RingCollectionId,
    ): Result<BandersnatchDomainSize> {
        return membersRepository.getCollection(
            chainId = chainId,
            collectionId = collectionId,
            consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
        ).map { it.ringSize.toDomainSize() }
    }
}
