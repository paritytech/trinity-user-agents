package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId

internal class CrossChainTransfersDirectionsConfiguration(
    val customTeleports: Set<CustomTeleportEntry>,
    val chains: Map<ChainId, List<AssetTransfers>>
) {
    class AssetTransfers(
        val assetId: ChainAssetId,
        val destinations: List<TransferDestination>
    )

    class TransferDestination(
        val fullChainAssetId: FullChainAssetId,
        val hasDeliveryFee: Boolean,
        val supportsXcmExecute: Boolean,
    )

    data class CustomTeleportEntry(
        val originChainAssetId: FullChainAssetId,
        val destinationChainId: ChainId
    )
}
