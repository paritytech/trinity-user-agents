import Foundation
import SubstrateSdk
import AssetHubSdk
import XcmDefinition
import BigInt
import AssetsManagement
import ChainRegistry

final class AssetHubTokenConverter {}

extension AssetHubTokenConverter: AssetHubTokenConverting {
    func convertToMultilocation(
        chainAsset: ChainAssetProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) -> AssetConversionPallet.AssetId? {
        guard
            let chainAssetModel = chainAsset as? ChainAsset,
            let storageInfo = try? AssetStorageInfo.extract(
                from: chainAssetModel.asset,
                codingFactory: codingFactory
            ) else {
            return nil
        }

        switch storageInfo {
        case .native:
            if chainAssetModel.chain.isUtilityTokenOnRelaychain {
                return .init(parents: 1, interior: .init(items: []))
            } else {
                return .init(parents: 0, interior: .init(items: []))
            }
        case let .statemine(info) where info.assetIdString.isHex():
            let remoteAssetId = try? info.assetId.map(
                to: AssetConversionPallet.AssetId.self,
                with: codingFactory.createRuntimeJsonContext().toRawContext()
            )

            return remoteAssetId
        case let .statemine(info):
            let palletName = info.palletName ?? AssetsPallet.name

            guard
                let palletIndex = codingFactory.metadata.getModuleIndex(palletName),
                let generalIndex = BigUInt(info.assetIdString) else {
                return nil
            }

            let palletJunction = XcmUni.Junction.palletInstance(palletIndex)
            let generalIndexJunction = XcmUni.Junction.generalIndex(generalIndex)

            return .init(parents: 0, interior: .init(items: [palletJunction, generalIndexJunction]))
        default:
            return nil
        }
    }

    func convertFromMultilocation(
        _ assetId: AssetConversionAssetIdProtocol,
        chain: ChainProtocol
    ) -> AssetConversionPallet.PoolAsset? {
        guard let chainModel = chain as? ChainModel else {
            return nil
        }

        let junctions = assetId.items

        if assetId.parents == 0 {
            guard !junctions.isEmpty else {
                return .native
            }

            switch junctions[0] {
            case let .palletInstance(pallet):
                if
                    junctions.count == 2,
                    case let .generalIndex(index) = junctions[1] {
                    return .assets(pallet: pallet, index: index)
                } else {
                    return .undefined(.init(parents: assetId.parents, interior: .init(items: junctions)))
                }
            default:
                return .undefined(.init(parents: assetId.parents, interior: .init(items: junctions)))
            }
        } else if assetId.parents == 1, junctions.isEmpty, chainModel.isUtilityTokenOnRelaychain {
            return .native
        } else {
            return .foreign(.init(parents: assetId.parents, interior: .init(items: junctions)))
        }
    }
}
