import Foundation
import SubstrateSdk
import AssetHubSdk
import AssetsManagement
import ChainRegistry

enum AssetHubBulkTokensMapperFactoryError: Error {
    case unexpectedChain(ChainProtocol)
}

final class AssetHubBulkTokensMapperFactory {}

extension AssetHubBulkTokensMapperFactory: AssetHubBulkTokensMapperFactoryProtocol {
    func createBulkMapper(
        for chain: ChainProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> AssetHubBulkTokensMapping {
        guard let chainModel = chain as? ChainModel else {
            throw AssetHubBulkTokensMapperFactoryError.unexpectedChain(chain)
        }

        return AssetHubBulkTokensMapper(chain: chainModel, codingFactory: codingFactory)
    }
}

final class AssetHubBulkTokensMapper {
    let chain: ChainModel
    let codingFactory: RuntimeCoderFactoryProtocol

    private let optNativeAsset: AssetModel?
    private let assetsPalletTokens: [JSON: (AssetModel, AssetsPalletStorageInfo)]

    init(chain: ChainModel, codingFactory: RuntimeCoderFactoryProtocol) {
        self.chain = chain
        self.codingFactory = codingFactory
        optNativeAsset = chain.utilityAsset()

        let initAssetsStore = [JSON: (AssetModel, AssetsPalletStorageInfo)]()
        assetsPalletTokens = chain.assets.reduce(into: initAssetsStore) { store, asset in
            let optStorageInfo = try? AssetStorageInfo.extract(from: asset, codingFactory: codingFactory)
            guard case let .statemine(info) = optStorageInfo else {
                return
            }

            store[info.assetId] = (asset, info)
        }
    }
}

extension AssetHubBulkTokensMapper: AssetHubBulkTokensMapping {
    func convertPoolAsset(_ asset: AssetConversionPallet.PoolAsset) -> ChainAssetId? {
        switch asset {
        case .native:
            if let nativeAsset = optNativeAsset {
                return ChainAssetId(chainId: chain.chainId, assetId: nativeAsset.assetId)
            } else {
                return nil
            }
        case let .assets(pallet, index):
            guard let localToken = assetsPalletTokens[.stringValue(String(index))] else {
                return nil
            }

            let palletName = localToken.1.palletName ?? AssetsPallet.name

            guard
                let moduleIndex = codingFactory.metadata.getModuleIndex(palletName),
                moduleIndex == pallet else {
                // only Assets pallet currently supported
                return nil
            }

            return ChainAssetId(chainId: chain.chainId, assetId: localToken.0.assetId)
        case let .foreign(remoteId):
            guard
                let json = try? remoteId.toScaleCompatibleJSON(),
                let localToken = assetsPalletTokens[json] else {
                return nil
            }

            return ChainAssetId(chainId: chain.chainId, assetId: localToken.0.assetId)
        default:
            return nil
        }
    }
}
