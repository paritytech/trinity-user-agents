import Foundation
import BigInt
import SubstrateSdk

public struct XcmUnweightedTransferRequest {
    public let origin: XcmTransferOrigin
    public let destination: XcmTransferDestination
    public let reserve: XcmTransferReserve
    public let metadata: XcmTransferMetadata
    public let amount: BigUInt

    public var originChain: ChainProtocol {
        origin.chainAsset.chainInterface
    }

    public var reserveChain: ChainProtocol {
        reserve.chain
    }

    public var destinationChain: ChainProtocol {
        destination.chain
    }

    public var isNativeAssetTransferBetweenSystemChains: Bool {
        origin.chainAsset.assetInterface.isUtility &&
            origin.parachainId.isSystemParachain &&
            destination.parachainId.isSystemParachain
    }

    public var isNonReserveTransfer: Bool {
        !isNativeAssetTransferBetweenSystemChains &&
            reserveChain.chainId != originChain.chainId && reserveChain.chainId != destinationChain.chainId
    }

    public var paraIdAfterOrigin: ParaId? {
        isNonReserveTransfer ? reserve.parachainId : destination.parachainId
    }

    public var paraIdBeforeDestination: ParaId? {
        isNonReserveTransfer ? reserve.parachainId : origin.parachainId
    }

    public init(
        origin: XcmTransferOrigin,
        destination: XcmTransferDestination,
        reserve: XcmTransferReserve,
        metadata: XcmTransferMetadata,
        amount: BigUInt
    ) {
        self.origin = origin
        self.destination = destination
        self.reserve = reserve
        self.metadata = metadata
        self.amount = amount
    }

    public func replacing(amount: Balance) -> XcmUnweightedTransferRequest {
        XcmUnweightedTransferRequest(
            origin: origin,
            destination: destination,
            reserve: reserve,
            metadata: metadata,
            amount: amount
        )
    }
}
