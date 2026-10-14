import Foundation
import SubstrateSdk

public struct XcmGeneralConfig: Decodable {
    public struct Chains: Decodable {
        public let parachainIds: [ChainId: ParaId]
    }

    public typealias AssetLocation = JSON
    public typealias AssetLocationId = String
    public typealias AssetIdKey = String

    public struct Assets: Decodable {
        public let assetsLocation: [AssetLocationId: AssetReserve]

        // By default, asset reserve id is equal to its symbol
        // This mapping allows to override that for cases like multiple reserves (Statemine & Polkadot for DOT)
        public let reserveIdOverrides: [ChainId: [AssetIdKey: AssetLocationId]]?
    }

    public struct AssetReserve: Decodable {
        public let chainId: ChainId
        public let assetId: AssetId
        public let multiLocation: AssetLocation
    }

    public let chains: Chains
    public let assets: Assets
}

public extension XcmGeneralConfig.Assets {
    func getReservePath(for chainAsset: ChainAssetProtocol) -> XcmAsset.ReservePath? {
        let chainId = chainAsset.chainInterface.chainId
        let assetIdKey = String(chainAsset.assetInterface.assetId)

        let overridenLocation = reserveIdOverrides?[chainId]?[assetIdKey]
        let assetLocationId = overridenLocation ?? chainAsset.assetInterface.symbol

        guard let path = assetsLocation[assetLocationId]?.multiLocation else {
            return nil
        }

        return XcmAsset.ReservePath(type: .relative, path: path)
    }

    func getReserveChainId(for chainAsset: ChainAssetProtocol) -> ChainId? {
        let chainId = chainAsset.chainInterface.chainId
        let assetIdKey = String(chainAsset.assetInterface.assetId)

        let assetLocationId = reserveIdOverrides?[chainId]?[assetIdKey] ?? chainAsset.assetInterface.symbol

        return assetsLocation[assetLocationId]?.chainId
    }
}
