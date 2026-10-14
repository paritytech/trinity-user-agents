package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.common.utils.graph.SimpleEdge
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDirection
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDirectionId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.AssetTransfers
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.CustomTeleportEntry
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.TransferDestination
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.XcmTransferType
import io.paritytech.polkadotapp.feature_xcm_api.chain.XcmChain
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.chainLocationOf

internal fun CrossChainTransfersConfiguration.availableOutDestinations(origin: Chain.Asset): List<FullChainAssetId> {
    val assetTransfers = outComingAssetTransfers(origin.fullId) ?: return emptyList()
    return assetTransfers.destinations.map { it.fullChainAssetId }
}

internal fun CrossChainTransfersConfiguration.availableDirectionIds(destination: Chain.Asset): List<FullChainAssetId> {
    val requiredDestinationId = destination.fullId

    return directions.chains.flatMap { (originChainId, chainTransfers) ->
        chainTransfers.mapNotNull { originAssetTransfers ->
            val hasDestination =
                originAssetTransfers.destinations.any { it.fullChainAssetId == requiredDestinationId }

            if (hasDestination) {
                FullChainAssetId(originChainId, originAssetTransfers.assetId)
            } else {
                null
            }
        }
    }
}

internal fun CrossChainTransfersConfiguration.availableDirectionIds(): List<Edge<FullChainAssetId>> {
    return directions.chains.flatMap { (originChainId, chainTransfers) ->
        chainTransfers.flatMap { originAssetTransfers ->
            originAssetTransfers.destinations.map {
                val from = FullChainAssetId(originChainId, originAssetTransfers.assetId)
                val to = it.fullChainAssetId

                SimpleEdge(from, to)
            }
        }
    }
}

internal fun CrossChainTransfersConfiguration.transferFeatures(directionId: CrossChainTransferDirectionId): CrossChainTransferFeatures? {
    return outComingAssetTransfers(directionId.from)?.getDestination(directionId.to.chainId)
        ?.getTransferFeatures()
}

internal suspend fun CrossChainTransfersConfiguration.transferConfiguration(
    direction: CrossChainTransferDirection,
): CrossChainTransferConfiguration? {
    val originChain = direction.from.chain
    val originXcmChain = getXcmChain(originChain)
    val originAsset = direction.from.asset

    val destinationChain = direction.to.chain
    val destinationXcmChain = getXcmChain(destinationChain)

    val assetTransfers = outComingAssetTransfers(originAsset.fullId) ?: return null
    val targetTransfer = assetTransfers.getDestination(destinationChain.id) ?: return null

    val reserve = reserveRegistry.getReserve(originAsset)
    val locationConverter = reserveRegistry.chainLocationConverter

    return CrossChainTransferConfiguration(
        originChain = originChain,
        destinationChain = destinationChain,
        originChainLocation = locationConverter.chainLocationOf(originChain),
        destinationChainLocation = locationConverter.chainLocationOf(destinationChain),
        originChainAsset = originAsset,
        transferType = XcmTransferType.determineTransferType(
            usesTeleports = canUseTeleport(originXcmChain, originAsset, destinationXcmChain),
            originChain = originChain,
            destinationChain = destinationChain,
            reserve = reserve
        ),
        features = targetTransfer.getTransferFeatures(),
    )
}

private fun CrossChainTransfersConfiguration.getXcmChain(chain: Chain): XcmChain {
    return XcmChain(
        chain = chain,
        parachainId = parachainIds[chain.id]
    )
}

private fun CrossChainTransfersConfiguration.canUseTeleport(
    originXcmChain: XcmChain,
    originAsset: Chain.Asset,
    destinationXcmChain: XcmChain,
): Boolean {
    val customTeleportEntry = CustomTeleportEntry(originAsset.fullId, destinationXcmChain.chain.id)
    if (customTeleportEntry in directions.customTeleports) return true

    return XcmTransferType.isSystemTeleport(originXcmChain, destinationXcmChain)
}

private fun AssetTransfers.getDestination(destinationChainId: ChainId): TransferDestination? {
    return destinations.find { it.fullChainAssetId.chainId == destinationChainId }
}

private fun TransferDestination.getTransferFeatures(): CrossChainTransferFeatures {
    return CrossChainTransferFeatures(
        hasDeliveryFees = hasDeliveryFee,
        supportsXcmExecute = supportsXcmExecute,
    )
}

private fun CrossChainTransfersConfiguration.outComingAssetTransfers(origin: FullChainAssetId): AssetTransfers? {
    return directions.chains[origin.chainId]?.find { it.assetId == origin.assetId }
}
