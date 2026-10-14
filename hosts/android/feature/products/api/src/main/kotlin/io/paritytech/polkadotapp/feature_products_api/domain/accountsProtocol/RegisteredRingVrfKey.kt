package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

/**
 * RFC-0024 registry entry as returned to a caller.
 *
 * [handle] names a slot in the owner's ring-VRF domain, not an sr25519 account, and is opaque to
 * consumers — they select by [rings], never by index.
 */
data class RegisteredRingVrfKey(
    val handle: ProductAccountId,
    val rings: List<RingLocation>,
    /** Present when the caller owns the key or holds a public-key disclosure grant. */
    val publicKey: DataByteArray?,
)
