package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

/**
 * A single stored registration: one key, declared for one ring. A key registered for several rings
 * is several of these, which is what makes extending an entry an insert rather than a rewrite.
 */
data class RingVrfKeyRegistration(
    val handle: ProductAccountId,
    val ring: RingLocation,
    val publicKey: DataByteArray,
)
