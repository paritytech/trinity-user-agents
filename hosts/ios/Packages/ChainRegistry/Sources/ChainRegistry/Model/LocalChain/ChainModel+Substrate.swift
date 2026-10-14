import Foundation
import SubstrateSdk
import Operation_iOS

extension ChainModel: ChainProtocol {
    public var genesisHash: ChainId {
        explicitGenesisHash ?? chainId
    }

    public var disabledCheckMetadataHash: Bool {
        true
    }

    public var base58Prefix: UInt16 {
        switch chainFormat {
        case let .substrate(addressPrefix):
            addressPrefix
        case .ethereum:
            42
        }
    }

    public func assetInteface(for assetId: AssetId) -> (any SubstrateSdk.AssetProtocol)? {
        asset(for: assetId)
    }

    public func chainAssetInterface(for assetId: SubstrateSdk.AssetId) -> ChainAssetProtocol? {
        chainAsset(for: assetId)
    }

    public func chainAssetsInterface() -> [any SubstrateSdk.ChainAssetProtocol] {
        chainAssets()
    }

    public func address(for accountId: AccountId) throws -> AccountAddress {
        try accountId.toAddress(using: chainFormat)
    }
}

extension AssetModel: AssetProtocol {}

extension ChainAsset: ChainAssetProtocol {
    public var chainInterface: ChainProtocol {
        chain
    }

    public var assetInterface: AssetProtocol {
        asset
    }

    public var chainAssetId: ChainAssetId {
        ChainAssetId(chainId: chain.chainId, assetId: asset.assetId)
    }
}

public enum ChainAssetConversionError: Error {
    case unsupportedAsset(String)
}

extension ChainAsset: Identifiable {
    public var identifier: String { chainAssetId.stringValue }
}
