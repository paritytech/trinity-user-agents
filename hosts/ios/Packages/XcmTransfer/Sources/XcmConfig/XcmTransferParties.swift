import Foundation
import SubstrateSdk

public struct XcmTransferParties {
    public let origin: XcmTransferOrigin
    public let destination: XcmTransferDestination
    public let reserve: XcmTransferReserve
    public let metadata: XcmTransferMetadata

    public var originChain: ChainProtocol {
        origin.chainAsset.chainInterface
    }

    public var reserveChain: ChainProtocol {
        reserve.chain
    }

    public var destinationChain: ChainProtocol {
        destination.chain
    }

    public init(
        origin: XcmTransferOrigin,
        destination: XcmTransferDestination,
        reserve: XcmTransferReserve,
        metadata: XcmTransferMetadata
    ) {
        self.origin = origin
        self.destination = destination
        self.reserve = reserve
        self.metadata = metadata
    }
}
