package io.paritytech.polkadotapp.feature_people_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_members_api.domain.CheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.toRingCollectionId
import javax.inject.Inject

class RealPeopleCheckMemberInRingUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val knownChains: KnownChains,
    private val checkMemberInRingUseCase: CheckMemberInRingUseCase,
) : PeopleCheckMemberInRingUseCase {
    override suspend fun awaitIncluded(peopleCollection: PeopleCollection): Result<Unit> {
        return resolveMemberSource(peopleCollection).flatMap { memberSource ->
            checkMemberInRingUseCase.awaitIncluded(knownChains.people, peopleCollection.toRingCollectionId(), memberSource)
        }
    }

    override suspend fun checkIncludes(peopleCollection: PeopleCollection): Result<Boolean> {
        return resolveMemberSource(peopleCollection).flatMap { memberSource ->
            checkMemberInRingUseCase.checkIncludes(knownChains.people, peopleCollection.toRingCollectionId(), memberSource)
        }
    }

    private suspend fun resolveMemberSource(peopleCollection: PeopleCollection): Result<MemberSource> = runCatching {
        val metaId = when (peopleCollection) {
            PeopleCollection.People -> accountRepository.getCandidateAccount().id
            PeopleCollection.LitePeople -> accountRepository.getWalletAccount().id
        }
        MemberSource.Account(metaId)
    }
}
