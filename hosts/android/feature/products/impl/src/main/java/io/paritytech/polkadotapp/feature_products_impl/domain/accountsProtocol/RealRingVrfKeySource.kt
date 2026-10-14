package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ringVrfPath
import io.paritytech.polkadotapp.feature_products_api.model.productIdTyped
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.ReservedRingVrfKeys
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistry
import javax.inject.Inject

class RealRingVrfKeySource @Inject constructor(
    private val registry: RingVrfKeyRegistry,
    private val reservedRingVrfKeys: ReservedRingVrfKeys,
    private val ringVrfEntropyDeriver: RingVrfEntropyDeriver,
    private val membersRingLocator: MembersRingLocator,
) : RingVrfKeySource {
    override suspend fun resolveMember(handle: ProductAccountId, ring: RingLocation): Result<MemberSource> {
        val declaredRings = registry.declaredRings(handle)

        if (declaredRings.isNotEmpty()) {
            if (ring !in declaredRings) return Result.failure(RingVrfKeyError.KeyNotInRing)

            return deriveEntropy(handle).map(MemberSource::Entropy)
        }

        if (!reservedRingVrfKeys.isReserved(handle)) {
            return Result.failure(RingVrfKeyError.KeyNotRegistered)
        }

        // The reserved keys predate the registry, so they resolve through the meta account that
        // already holds their entropy rather than being re-derived.
        return membersRingLocator.locateRing(ring).map { located -> MemberSource.Account(located.metaId) }
    }

    override suspend fun resolveEntropy(handle: ProductAccountId): Result<BandersnatchEntropy> {
        val known = registry.declaredRings(handle).isNotEmpty() || reservedRingVrfKeys.isReserved(handle)

        return if (known) {
            deriveEntropy(handle)
        } else {
            Result.failure(RingVrfKeyError.KeyNotRegistered)
        }
    }

    private suspend fun deriveEntropy(handle: ProductAccountId): Result<BandersnatchEntropy> = runCatching {
        ringVrfEntropyDeriver.deriveRingVrfEntropy(ringVrfPath(handle.productIdTyped, handle.index))
    }
}
