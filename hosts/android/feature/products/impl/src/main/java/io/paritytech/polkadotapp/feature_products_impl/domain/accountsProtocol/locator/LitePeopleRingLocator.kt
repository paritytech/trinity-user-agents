package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE_LITE
import javax.inject.Inject

internal class LitePeopleRingLocator @Inject constructor(
    private val accountRepository: AccountRepository,
) : MembersRingLocatorStrategy {
    override fun appliesTo(requestedCollectionId: RingCollectionId?): Boolean =
        requestedCollectionId == RingCollectionId.PEOPLE_LITE

    override fun resolveCollectionId(requestedCollectionId: RingCollectionId?): RingCollectionId =
        RingCollectionId.PEOPLE_LITE

    override suspend fun resolveMetaId(): Long = accountRepository.getWalletAccount().id
}
