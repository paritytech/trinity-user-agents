package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import javax.inject.Inject

// Terminal strategy: any unrecognised or absent collection falls back to the requested
// collection (or the PoP People ring) proven with the full-people (candidate) member key.
internal class FallbackPeopleRingLocator @Inject constructor(
    private val accountRepository: AccountRepository,
) : MembersRingLocatorStrategy {
    override fun appliesTo(requestedCollectionId: RingCollectionId?): Boolean = true

    override fun resolveCollectionId(requestedCollectionId: RingCollectionId?): RingCollectionId =
        requestedCollectionId ?: RingCollectionId.PEOPLE

    override suspend fun resolveMetaId(): Long = accountRepository.getCandidateAccount().id
}
