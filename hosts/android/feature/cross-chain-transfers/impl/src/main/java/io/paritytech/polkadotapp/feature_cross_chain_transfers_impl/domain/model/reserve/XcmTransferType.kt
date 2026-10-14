package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_xcm_api.chain.XcmChain
import io.paritytech.polkadotapp.feature_xcm_api.chain.isRelay
import io.paritytech.polkadotapp.feature_xcm_api.chain.isSystemChain
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation

internal sealed interface XcmTransferType {
    companion object {
        fun determineTransferType(
            usesTeleports: Boolean,
            originChain: Chain,
            destinationChain: Chain,
            reserve: TokenReserve
        ): XcmTransferType {
            val assetAbsoluteLocation = reserve.tokenLocation

            return when {
                usesTeleports -> Teleport(assetAbsoluteLocation)
                originChain.id == reserve.reserveChainLocation.chainId -> Reserve.Origin(assetAbsoluteLocation)
                destinationChain.id == reserve.reserveChainLocation.chainId -> Reserve.Destination(assetAbsoluteLocation)
                else -> Reserve.Remote(assetAbsoluteLocation, reserve.reserveChainLocation)
            }
        }

        fun isSystemTeleport(originXcmChain: XcmChain, destinationXcmChain: XcmChain): Boolean {
            val systemToRelay = originXcmChain.isSystemChain() && destinationXcmChain.isRelay()
            val relayToSystem = originXcmChain.isRelay() && destinationXcmChain.isSystemChain()
            val systemToSystem = originXcmChain.isSystemChain() && destinationXcmChain.isSystemChain()

            return systemToRelay || relayToSystem || systemToSystem
        }
    }

    val assetAbsoluteLocation: AbsoluteMultiLocation

    data class Teleport(override val assetAbsoluteLocation: AbsoluteMultiLocation) : XcmTransferType

    sealed interface Reserve : XcmTransferType {
        data class Origin(override val assetAbsoluteLocation: AbsoluteMultiLocation) : Reserve

        data class Destination(override val assetAbsoluteLocation: AbsoluteMultiLocation) : Reserve

        data class Remote(
            override val assetAbsoluteLocation: AbsoluteMultiLocation,
            val remoteReserveLocation: ChainLocation
        ) : Reserve
    }
}

internal fun XcmTransferType.remoteReserveLocation(): ChainLocation? {
    return (this as? XcmTransferType.Reserve.Remote)?.remoteReserveLocation
}

internal fun XcmTransferType.isRemoteReserve(): Boolean {
    return this is XcmTransferType.Reserve.Remote
}
