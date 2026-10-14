package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId

/**
 * Resolves a [RingLocation] to the inputs the membership prover needs, owning all
 * validation of the location (chain resolution, pallet-instance check, collection recognition).
 * New ring kinds are added as a new locator strategy rather than by branching at the call site.
 */
interface MembersRingLocator {
    suspend fun locateRing(ring: RingLocation): Result<LocatedRing>
}

// Neutral failure of ring resolution — mapped by each caller into its own error set
// (CreateProofError / GetAliasError), so the locator isn't tied to one consumer's type.
sealed class RingLocationError(message: String) : Throwable(message) {
    data object RingNotFound : RingLocationError("Ring not found for the requested location")
}

data class LocatedRing(
    val chainId: ChainId,
    val collectionId: RingCollectionId,
    val metaId: Long,
)
