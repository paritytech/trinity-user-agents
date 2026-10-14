package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId

internal interface MembersRingLocatorStrategy {
    fun appliesTo(requestedCollectionId: RingCollectionId?): Boolean
    fun resolveCollectionId(requestedCollectionId: RingCollectionId?): RingCollectionId
    suspend fun resolveMetaId(): Long
}
