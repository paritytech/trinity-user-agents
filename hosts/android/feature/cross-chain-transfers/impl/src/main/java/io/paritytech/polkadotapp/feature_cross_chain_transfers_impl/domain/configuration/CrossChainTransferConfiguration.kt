package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.XcmTransferType
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation

internal class CrossChainTransferConfiguration(
    val originChain: Chain,
    val destinationChain: Chain,
    val originChainLocation: ChainLocation,
    val destinationChainLocation: ChainLocation,
    val transferType: XcmTransferType,
    val originChainAsset: Chain.Asset,
    val features: CrossChainTransferFeatures,
) {
    override fun toString(): String {
        return """
            Direction: ${originChainAsset.symbol} ${originChain.name} -> ${destinationChain.name}"
            Transfer type: $transferType
            Features: $features
        """.trimIndent()
    }
}

internal val CrossChainTransferConfiguration.originChainId: ChainId
    get() = originChain.id

internal val CrossChainTransferConfiguration.destinationChainId: ChainId
    get() = destinationChain.id

internal fun CrossChainTransferConfiguration.assetLocationOnOrigin(): RelativeMultiLocation {
    return transferType.assetAbsoluteLocation.fromPointOfViewOf(originChainLocation.location)
}

internal fun CrossChainTransferConfiguration.destinationChainLocationOnOrigin(): RelativeMultiLocation {
    return destinationChainLocation.location.fromPointOfViewOf(originChainLocation.location)
}
