package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import javax.inject.Inject

internal class FullPeopleRingLocator @Inject constructor(
    private val accountRepository: AccountRepository,
) : MembersRingLocatorStrategy {
    override fun appliesTo(requestedCollectionId: RingCollectionId?): Boolean =
        requestedCollectionId == RingCollectionId.PEOPLE

    override fun resolveCollectionId(requestedCollectionId: RingCollectionId?): RingCollectionId =
        RingCollectionId.PEOPLE

    override suspend fun resolveMetaId(): Long = accountRepository.getCandidateAccount().id
}
