package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry

import io.novasama.substrate_sdk_android.runtime.metadata.moduleOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE_LITE
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import javax.inject.Inject

// Mirrors the personhood indices wired in ProductDerivationModule. RFC-0022 pins index 0 as the full
// personhood key and index 1 as the light one; under RFC-0024 they are registry entries the app owns
// on peopl.dot's behalf until a personhood product exists to register them itself.
private val FULL_PERSONHOOD_INDEX = DerivationIndex32.fromUInt(0u)
private val LIGHT_PERSONHOOD_INDEX = DerivationIndex32.fromUInt(1u)

/**
 * The registry entries the app answers for without anyone having registered them.
 *
 * Synthesized on read rather than seeded at startup: building a [RingLocation] needs the MEMBERS
 * pallet index from runtime metadata, which is not available before the chain is reachable.
 */
class ReservedRingVrfKeys @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    suspend fun isReserved(handle: ProductAccountId): Boolean {
        if (handle.productId != personhoodId().value) return false

        return handle.index == FULL_PERSONHOOD_INDEX || handle.index == LIGHT_PERSONHOOD_INDEX
    }

    private suspend fun ownedBy(owner: ProductId): Boolean = owner == personhoodId()

    private suspend fun personhoodId(): ProductId {
        return ReservedProductIds.personhood(dotNsTldProvider.getTldRetrying())
    }

    /**
     * Fails rather than returning an empty list when the chain cannot be read, so a caller can tell
     * "this user has no personhood key" apart from "the ring could not be resolved".
     */
    suspend fun entriesOf(owner: ProductId): Result<List<ReservedRingVrfKeyEntry>> {
        if (!ownedBy(owner)) return Result.success(emptyList())

        return runCatching {
            val peopleChain = chainRegistry.peopleChain()

            val chainId = chainRegistry.knownChains.people

            val membersPalletIndex = chainRegistry.getRuntime(chainId).metadata
                .moduleOrNull(Modules.MEMBERS)?.index?.toInt()?.toUByte()
                ?: error("MEMBERS pallet is absent from ${peopleChain.name} metadata")

            listOf(
                reservedEntry(owner, peopleChain.genesisHash, membersPalletIndex, FULL_PERSONHOOD_INDEX, RingCollectionId.PEOPLE),
                reservedEntry(owner, peopleChain.genesisHash, membersPalletIndex, LIGHT_PERSONHOOD_INDEX, RingCollectionId.PEOPLE_LITE),
            )
        }
    }

    private fun reservedEntry(
        owner: ProductId,
        genesisHash: GenesisHash,
        membersPalletIndex: UByte,
        index: DerivationIndex32,
        collectionId: RingCollectionId,
    ): ReservedRingVrfKeyEntry {
        val ring = RingLocation(
            chainId = genesisHash,
            junctions = listOf(
                RingLocationJunction.PalletInstance(membersPalletIndex),
                RingLocationJunction.CollectionId(collectionId.value),
            )
        )

        return ReservedRingVrfKeyEntry(
            handle = ProductAccountId(owner.value, index),
            ring = ring,
        )
    }
}

class ReservedRingVrfKeyEntry(
    val handle: ProductAccountId,
    val ring: RingLocation,
)
