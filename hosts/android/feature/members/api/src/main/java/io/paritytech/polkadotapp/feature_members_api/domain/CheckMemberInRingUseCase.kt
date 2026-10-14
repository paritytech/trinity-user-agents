package io.paritytech.polkadotapp.feature_members_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource

interface CheckMemberInRingUseCase {
    /**
     * Suspends until [memberSource] for [collectionId] has been fully included in its ring —
     * i.e. it is safe to submit ring proofs on-chain via `MembershipProver` for this member.
     */
    suspend fun awaitIncluded(
        chainId: ChainId,
        collectionId: RingCollectionId,
        memberSource: MemberSource,
    ): Result<Unit>

    /**
     * One-shot check of the same condition as [awaitIncluded] against the latest committed state.
     * `true` means ring proofs for [memberSource] in [collectionId] are accepted on-chain now.
     */
    suspend fun checkIncludes(
        chainId: ChainId,
        collectionId: RingCollectionId,
        memberSource: MemberSource,
    ): Result<Boolean>
}
