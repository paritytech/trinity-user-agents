package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry

import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ringVrfPath
import io.paritytech.polkadotapp.feature_products_api.model.productIdTyped
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RingVrfKeyRegistrationRepository
import javax.inject.Inject

class RealRingVrfKeyRegistry @Inject constructor(
    private val repository: RingVrfKeyRegistrationRepository,
    private val ringVrfEntropyDeriver: RingVrfEntropyDeriver,
    private val reservedRingVrfKeys: ReservedRingVrfKeys,
) : RingVrfKeyRegistry {
    override suspend fun register(
        owner: ProductId,
        index: DerivationIndex32,
        ring: RingLocation,
    ): Result<DataByteArray> = runCatching {
        val handle = ProductAccountId(owner.value, index)
        val publicKey = memberPublicKeyOf(handle)

        repository.save(RingVrfKeyRegistration(handle = handle, ring = ring, publicKey = publicKey))

        publicKey
    }

    override suspend fun list(owner: ProductId): Result<List<RegisteredRingVrfKey>> {
        return reservedRingVrfKeys.entriesOf(owner).flatMap { reserved ->
            runCatching {
                val stored = storedEntriesOf(owner)

                stored + reservedEntriesOf(reserved, shadowedBy = stored)
            }
        }
    }

    override suspend fun declaredRings(handle: ProductAccountId): List<RingLocation> {
        return repository.getByHandle(handle).map { it.ring }
    }

    private suspend fun storedEntriesOf(owner: ProductId): List<RegisteredRingVrfKey> {
        return repository.getByOwner(owner)
            .groupBy { it.handle }
            .map { (handle, registrations) ->
                RegisteredRingVrfKey(
                    handle = handle,
                    rings = registrations.map { it.ring },
                    publicKey = registrations.first().publicKey,
                )
            }
    }

    /** A real registration shadows the entry the app synthesized for the same handle. */
    private suspend fun reservedEntriesOf(
        reserved: List<ReservedRingVrfKeyEntry>,
        shadowedBy: List<RegisteredRingVrfKey>,
    ): List<RegisteredRingVrfKey> {
        val shadowed = shadowedBy.map { it.handle }.toSet()

        return reserved
            .filterNot { it.handle in shadowed }
            .groupBy { it.handle }
            .map { (handle, entries) ->
                RegisteredRingVrfKey(
                    handle = handle,
                    rings = entries.map { it.ring },
                    publicKey = memberPublicKeyOf(handle),
                )
            }
    }

    private suspend fun memberPublicKeyOf(handle: ProductAccountId): DataByteArray {
        return ringVrfEntropyDeriver.deriveRingVrfEntropy(ringVrfPath(handle.productIdTyped, handle.index)).memberKey()
    }
}
