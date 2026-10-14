import Foundation
import SubstrateSdk

public struct XcmTransferDestination {
    public let chainAsset: ChainAssetProtocol
    public let parachainId: ParaId?
    public let accountId: AccountId

    public var chain: ChainProtocol {
        chainAsset.chainInterface
    }

    public func replacing(accountId: AccountId) -> XcmTransferDestination {
        XcmTransferDestination(
            chainAsset: chainAsset,
            parachainId: parachainId,
            accountId: accountId
        )
    }

    public init(chainAsset: ChainAssetProtocol, parachainId: ParaId?, accountId: AccountId) {
        self.chainAsset = chainAsset
        self.parachainId = parachainId
        self.accountId = accountId
    }
}

public struct XcmTransferDestinationId {
    public let chainAssetId: ChainAssetId
    public let accountId: AccountId

    public var chainId: ChainId {
        chainAssetId.chainId
    }

    public init(chainAssetId: ChainAssetId, accountId: AccountId) {
        self.chainAssetId = chainAssetId
        self.accountId = accountId
    }
}
