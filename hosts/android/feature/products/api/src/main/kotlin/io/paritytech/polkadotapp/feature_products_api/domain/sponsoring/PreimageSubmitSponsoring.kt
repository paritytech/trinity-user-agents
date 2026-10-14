package io.paritytech.polkadotapp.feature_products_api.domain.sponsoring

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Sponsoring hook invoked before storing a preimage on the Bullet-In chain. Ensures
 * a valid Bulletin slot key exists for the calling product and that the on-chain
 * allowance for that slot has capacity for [size]; tops up if not. Returns the slot
 * key the caller should sign the `store` extrinsic with.
 */
interface PreimageSubmitSponsoring {
    suspend fun sponsorPreimageSubmit(productId: ProductId, size: InformationSize): Result<SlotAccountKey>
}
