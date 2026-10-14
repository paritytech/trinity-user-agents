package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.common.domain.model.DataByteArray

data class RingLocation(
    val chainId: GenesisHash,
    val junctions: List<RingLocationJunction>,
)

sealed interface RingLocationJunction {
    data class PalletInstance(val index: UByte) : RingLocationJunction
    data class CollectionId(val bytes: DataByteArray) : RingLocationJunction
}
