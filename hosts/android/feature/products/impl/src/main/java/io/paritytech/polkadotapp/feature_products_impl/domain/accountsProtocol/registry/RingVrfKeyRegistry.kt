package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * RFC-0024 key registry. The Account Holder is the authority, so this is the only place that decides
 * whether a `(product, index)` pair names a key that exists.
 */
interface RingVrfKeyRegistry {
    /**
     * Registers [index] of [owner] for [ring], returning the member public key. Idempotent, and
     * registering a known index for a further ring extends the entry rather than replacing it.
     */
    suspend fun register(owner: ProductId, index: DerivationIndex32, ring: RingLocation): Result<DataByteArray>

    /** Registry entries owned by [owner], including the reserved personhood entries. */
    suspend fun list(owner: ProductId): Result<List<RegisteredRingVrfKey>>

    /**
     * Rings [handle] was explicitly registered for. Reads storage only, so an empty list means the
     * handle has no stored registration — it may still be servable as a reserved key.
     */
    suspend fun declaredRings(handle: ProductAccountId): List<RingLocation>
}
