import Foundation
import HydrationSdk
import SubstrateSdk
import ChainRegistry

enum HydrationTokenConverterError: Error {
    case unexpectedChain(ChainProtocol)
    case unexpectedChainAsset(ChainAssetProtocol)
}

struct HydrationTokenConverter {}

extension HydrationTokenConverter: HydrationTokenConverting {
    func convertToRemoteLocalMapping(
        remoteAssets: Set<HydraDx.AssetId>,
        chain: ChainProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> [HydraDx.AssetId: ChainAssetId] {
        guard let chainModel = chain as? ChainModel else {
            throw HydrationTokenConverterError.unexpectedChain(chain)
        }

        let assetsMapping: [HydraDx.AssetId: ChainAssetId] = chainModel.assets.reduce(into: [:]) { accum, asset in
            switch AssetType(rawType: asset.type) {
            case .orml,
                 .ormlHydrationEvm:
                if let currencyId: StringCodable<HydraDx.AssetId> = try? asset.getOrmlCurrencyId(
                    for: codingFactory
                ) {
                    accum[currencyId.wrappedValue] = ChainAssetId(
                        chainId: chain.chainId,
                        assetId: asset.assetId
                    )
                }
            case .none,
                 .native:
                accum[HydraDx.nativeAssetId] = ChainAssetId(
                    chainId: chain.chainId,
                    assetId: asset.assetId
                )
            case .statemine:
                return
            }
        }

        return assetsMapping.filter { remoteAssets.contains($0.key) }
    }

    func convertToRemote(
        chainAsset: ChainAssetProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> HydraDx.LocalRemoteAssetId {
        guard let chainAssetModel = chainAsset as? ChainAsset else {
            throw HydrationTokenConverterError.unexpectedChainAsset(chainAsset)
        }

        let storageInfo = try AssetStorageInfo.extract(
            from: chainAssetModel.asset,
            codingFactory: codingFactory
        )

        switch storageInfo {
        case .native:
            return .init(localAssetId: chainAsset.chainAssetId, remoteAssetId: HydraDx.nativeAssetId)
        case let .orml(info),
             let .ormlHydrationEvm(info):
            let context = codingFactory.createRuntimeJsonContext()
            let remoteId = try info.currencyId.map(
                to: StringScaleMapper<HydraDx.AssetId>.self,
                with: context.toRawContext()
            ).value

            return .init(localAssetId: chainAsset.chainAssetId, remoteAssetId: remoteId)
        default:
            throw HydrationTokenConverterError.unexpectedChainAsset(chainAsset)
        }
    }
}
