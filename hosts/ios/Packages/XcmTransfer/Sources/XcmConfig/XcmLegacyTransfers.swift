import Foundation
import SubstrateSdk
import BigInt

public struct XcmLegacyTransfers: Decodable {
    let assetsLocation: [String: JSON]
    let instructions: [String: [String]]
    let networkDeliveryFee: [String: JSON]?
    let networkBaseWeight: [String: String]
    let chains: [XcmChain]

    public func assetLocation(for key: String) -> JSON? {
        assetsLocation[key]
    }

    public func instructions(for key: String) -> [String]? {
        instructions[key]
    }

    public func baseWeight(for chainId: String) -> BigUInt? {
        guard let baseWeight = networkBaseWeight[chainId] else {
            return nil
        }

        return BigUInt(baseWeight)
    }

    public func getReservePath(for chainAssetId: ChainAssetId) -> XcmAsset.ReservePath? {
        guard let asset = asset(from: chainAssetId) else {
            return nil
        }

        guard let assetLocation = assetLocation(for: asset.assetLocation)?.multiLocation else {
            return nil
        }

        switch asset.assetLocationPath.type {
        case .absolute,
             .relative:
            return XcmAsset.ReservePath(type: asset.assetLocationPath.type, path: assetLocation)
        case .concrete:
            if let concretePath = asset.assetLocationPath.path {
                return XcmAsset.ReservePath(type: .concrete, path: concretePath)
            } else {
                return nil
            }
        }
    }

    public func transferableAssetIds(from chainId: ChainId) -> Set<AssetId> {
        guard let chain = chains.first(where: { $0.chainId == chainId }) else {
            return Set()
        }

        let assetIds = chain.assets.map(\.assetId)
        return Set(assetIds)
    }

    public func getReserveChainId(for chainAssetId: ChainAssetId) -> ChainId? {
        guard
            let chain = chains.first(where: { $0.chainId == chainAssetId.chainId }),
            let asset = chain.assets.first(where: { $0.assetId == chainAssetId.assetId }),
            let assetLocation = assetsLocation[asset.assetLocation] else {
            return nil
        }

        return assetLocation.chainId?.stringValue
    }

    public func asset(from chainAssetId: ChainAssetId) -> XcmAsset? {
        guard let chain = chains.first(where: { $0.chainId == chainAssetId.chainId }) else {
            return nil
        }

        return chain.assets.first(where: { $0.assetId == chainAssetId.assetId })
    }

    func transfers(from chainAssetId: ChainAssetId) -> [XcmAssetTransfer] {
        guard
            let chain = chains.first(where: { $0.chainId == chainAssetId.chainId }),
            let xcmTransfers = chain.assets.first(where: { $0.assetId == chainAssetId.assetId })?.xcmTransfers else {
            return []
        }

        return xcmTransfers
    }

    public func transferChainAssets(to chainAssetId: ChainAssetId) -> [ChainAssetId] {
        chains.flatMap { chain in
            chain.assets.filter { asset in
                asset.xcmTransfers.contains(where: { transfer in
                    transfer.destination.chainId == chainAssetId.chainId &&
                        transfer.destination.assetId == chainAssetId.assetId
                })
            }.map {
                ChainAssetId(chainId: chain.chainId, assetId: $0.assetId)
            }
        }
    }

    public func transfer(
        from chainAssetId: ChainAssetId,
        destinationChainId: ChainId
    ) -> XcmAssetTransfer? {
        guard
            let chain = chains.first(where: { $0.chainId == chainAssetId.chainId }),
            let xcmTransfers = chain.assets.first(where: { $0.assetId == chainAssetId.assetId })?.xcmTransfers else {
            return nil
        }

        return xcmTransfers.first { $0.destination.chainId == destinationChainId }
    }

    public func destinationFee(
        from chainAssetId: ChainAssetId,
        to destinationChainId: ChainId
    ) -> XcmAssetTransferFee? {
        let transfer = transfer(from: chainAssetId, destinationChainId: destinationChainId)
        return transfer?.destination.fee
    }

    public func reserveFee(from chainAssetId: ChainAssetId) -> XcmAssetTransferFee? {
        guard
            let assetLocationId = asset(from: chainAssetId)?.assetLocation,
            let assetLocation = assetLocation(for: assetLocationId) else {
            return nil
        }

        return try? assetLocation.reserveFee?.map(to: XcmAssetTransferFee.self, with: nil)
    }

    public func deliveryFee(from chainId: ChainId) throws -> XcmDeliveryFee? {
        guard let deliveryFee = networkDeliveryFee?[chainId] else {
            return nil
        }

        return try deliveryFee.map(to: XcmDeliveryFee.self, with: nil)
    }
}

public extension XcmLegacyTransfers {
    static var empty: XcmLegacyTransfers {
        XcmLegacyTransfers(
            assetsLocation: [:],
            instructions: [:],
            networkDeliveryFee: [:],
            networkBaseWeight: [:],
            chains: []
        )
    }
}
