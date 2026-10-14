package io.paritytech.polkadotapp.feature_people_api.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision

interface PeopleMembershipProver {
    suspend fun proofPersonMembership(
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        peopleCollection: PeopleCollection,
        at: BlockHash? = null,
    ): Result<PeopleMembershipProof>

    /**
     * Resolves the active person's ring position and ring data once, so a caller producing several
     * proofs for the same person pays those remote reads once instead of once per proof. Reads are
     * pinned to [at] when given, the current best block otherwise; the returned prover keeps that pin.
     */
    suspend fun precomputeForMember(
        chainId: ChainId,
        peopleCollection: PeopleCollection,
        at: BlockHash? = null,
    ): Result<PrecomputedPersonMembershipProver>
}

/** The active person's ring position and ring data, resolved once and reused across proofs. */
interface PrecomputedPersonMembershipProver {
    suspend fun proofPersonMembership(
        message: ByteArray,
        context: BandersnatchContext,
    ): Result<PeopleMembershipProof>
}

class PeopleMembershipProof(
    val proof: BandersnatchProof,
    val ringIndex: RingIndex,
    val revision: RingRevision,
)
