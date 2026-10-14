package io.paritytech.polkadotapp.feature_people_impl.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_members_api.domain.MembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.PrecomputedMemberMembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProof
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.PrecomputedPersonMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.toRingCollectionId
import javax.inject.Inject

class RealPeopleMembershipProver @Inject constructor(
    private val accountRepository: AccountRepository,
    private val membershipProver: MembershipProver,
) : PeopleMembershipProver {
    override suspend fun proofPersonMembership(
        message: ByteArray,
        context: BandersnatchContext,
        chainId: ChainId,
        peopleCollection: PeopleCollection,
        at: BlockHash?,
    ): Result<PeopleMembershipProof> {
        return precomputeForMember(chainId, peopleCollection, at)
            .flatMap { it.proofPersonMembership(message, context) }
    }

    override suspend fun precomputeForMember(
        chainId: ChainId,
        peopleCollection: PeopleCollection,
        at: BlockHash?,
    ): Result<PrecomputedPersonMembershipProver> {
        val metaId = resolveMetaId(peopleCollection)

        return membershipProver.precomputeForMember(
            member = MemberSource.Account(metaId),
            chainId = chainId,
            collectionId = peopleCollection.toRingCollectionId(),
            blockHash = at,
        ).map(::PrecomputedPerson)
    }

    private class PrecomputedPerson(
        private val member: PrecomputedMemberMembershipProver,
    ) : PrecomputedPersonMembershipProver {
        override suspend fun proofPersonMembership(
            message: ByteArray,
            context: BandersnatchContext,
        ): Result<PeopleMembershipProof> {
            return member.createRingVrfProof(message, context)
                .map { result -> PeopleMembershipProof(result.proof, result.ringIndex, result.ringRevision) }
        }
    }

    private suspend fun resolveMetaId(peopleCollection: PeopleCollection): Long = when (peopleCollection) {
        PeopleCollection.People -> accountRepository.getCandidateAccount().id
        PeopleCollection.LitePeople -> accountRepository.getWalletAccount().id
    }
}
