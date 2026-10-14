package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.novasama.substrate_sdk_android.runtime.metadata.moduleOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.LocatedRing
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import javax.inject.Inject

internal class RealMembersRingLocator @Inject constructor(
    private val chainRegistry: ChainRegistry,
    fullPeopleRingLocator: FullPeopleRingLocator,
    litePeopleRingLocator: LitePeopleRingLocator,
    fallbackPeopleRingLocator: FallbackPeopleRingLocator,
) : MembersRingLocator {
    private val strategies: List<MembersRingLocatorStrategy> = listOf(
        fullPeopleRingLocator,
        litePeopleRingLocator,
        fallbackPeopleRingLocator,
    )

    override suspend fun locateRing(ring: RingLocation): Result<LocatedRing> = runCatching {
        val chainId = chainRegistry.getChainIdByGenesisHash(ring.chainId)
            ?: throw RingLocationError.RingNotFound

        validatePalletInstance(ring, chainId)

        val requestedCollectionId = ring.requestedCollectionId()
        val strategy = strategies.first { it.appliesTo(requestedCollectionId) }

        LocatedRing(
            chainId = chainId,
            collectionId = strategy.resolveCollectionId(requestedCollectionId),
            metaId = strategy.resolveMetaId(),
        )
    }

    private suspend fun validatePalletInstance(ring: RingLocation, chainId: ChainId) {
        val palletInstance = ring.junctions
            .filterIsInstance<RingLocationJunction.PalletInstance>()
            .firstOrNull() ?: return

        val membersIndex = chainRegistry.getRuntime(chainId).metadata
            .moduleOrNull(Modules.MEMBERS)?.index?.toInt()
            ?: throw RingLocationError.RingNotFound

        if (palletInstance.index.toInt() != membersIndex) {
            throw RingLocationError.RingNotFound
        }
    }

    private fun RingLocation.requestedCollectionId(): RingCollectionId? {
        val bytes = junctions
            .filterIsInstance<RingLocationJunction.CollectionId>()
            .firstOrNull()?.bytes ?: return null

        return RingCollectionId(bytes)
    }
}
