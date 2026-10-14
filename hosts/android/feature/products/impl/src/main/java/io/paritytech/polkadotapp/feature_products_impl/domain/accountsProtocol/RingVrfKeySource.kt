package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

/**
 * RFC-0024 replacement for host-side member-key selection: a key handle, not a ring, decides which
 * key answers. Ring resolution stays with [MembersRingLocator]; only key selection moves here.
 */
interface RingVrfKeySource {
    /**
     * Member backing [handle] for a proof or alias within [ring]. A registered handle is derived
     * from its path; a reserved personhood handle with no row falls back to the embedded locator, so
     * the app's own flows keep using the entropy already cached against their meta account.
     */
    suspend fun resolveMember(handle: ProductAccountId, ring: RingLocation): Result<MemberSource>

    /**
     * Entropy backing [handle] for a plain signature. There is no ring to check — `ring_vrf_sign`
     * derives no alias and proves no membership.
     */
    suspend fun resolveEntropy(handle: ProductAccountId): Result<BandersnatchEntropy>
}

/**
 * Neutral failures of handle resolution, mapped by each caller into its own error set the way
 * [io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationError] already is.
 */
sealed class RingVrfKeyError(message: String) : Throwable(message) {
    data object KeyNotRegistered : RingVrfKeyError("Key handle has no registry entry")
    data object KeyNotInRing : RingVrfKeyError("Key handle is registered, but not for the requested ring")
}
