package io.paritytech.polkadotapp.feature_members_api.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProofResult
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource

interface MembershipProver {
    suspend fun proofMembership(
        member: MemberSource,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash? = null,
    ): Result<BandersnatchProofResult>

    suspend fun proofMembershipBatched(
        members: List<MemberSource>,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        ringIndex: RingIndex,
        blockHash: BlockHash? = null,
    ): Result<List<BandersnatchProof>>

    /**
     * Unlike [proofMembership], which takes a known [RingIndex] and returns only the raw proof, this
     * resolves the member's [RingIndex] (from on-chain membership) and the ring's [RingRevision]
     * itself and returns the full [RingVrfMembershipProof] (proof + alias + index + revision). It
     * pins every read to a single block — [blockHash] when given, the current best block otherwise —
     * so the index, revision and proof are mutually consistent.
     * Fails with [RingVrfProofError.NotMember] when the member is not in the ring, or
     * [RingVrfProofError.RingNotFound] when the ring root is missing.
     */
    suspend fun createRingVrfProof(
        member: MemberSource,
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        collectionId: RingCollectionId,
        blockHash: BlockHash? = null,
    ): Result<RingVrfMembershipProof>

    /**
     * Resolves [member]'s ring index, the ring's [RingRevision] and the ring data once, so a caller
     * producing several proofs for the same member pays those remote reads once instead of once per
     * proof. Equivalent to [createRingVrfProof] split into its data half and its proving half. Reads
     * are pinned to [blockHash] when given, the current best block otherwise; the returned prover keeps
     * that pin for its whole lifetime.
     * Fails with [RingVrfProofError.NotMember] when the member is not in the ring, or
     * [RingVrfProofError.RingNotFound] when the ring root is missing.
     */
    suspend fun precomputeForMember(
        member: MemberSource,
        chainId: ChainId,
        collectionId: RingCollectionId,
        blockHash: BlockHash? = null,
    ): Result<PrecomputedMemberMembershipProver>
}

/** Ring data plus one member's position in it, resolved once and reused across proofs. */
interface PrecomputedMemberMembershipProver {
    suspend fun createRingVrfProof(
        message: ByteArray,
        context: BandersnatchContext,
    ): Result<RingVrfMembershipProof>
}
