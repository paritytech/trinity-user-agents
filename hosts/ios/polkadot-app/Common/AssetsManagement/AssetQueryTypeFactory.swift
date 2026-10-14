import AssetsManagement
import Foundation
import SubstrateSdk
import ChainRegistry

final class AssetQueryTypeFactory: AssetQueryTypeMaking {
    func deriveQueryType(_ chainAsset: ChainAssetProtocol) -> AssetQueryType? {
        guard let assetModel = chainAsset.assetInterface as? AssetModel else {
            return nil
        }

        let mapper = CustomAssetMapper(
            type: assetModel.type,
            typeExtras: assetModel.typeExtras
        )

        return try? mapper.mapAssetWithExtras(.init(
            nativeHandler: { .native },
            statemineHandler: { .statemine(assetId: $0.assetId, palletName: $0.palletName) },
            ormlHandler: { .orml(currencyIdScale: $0.currencyIdScale) },
            ormlHydrationEvmHandler: { _ in nil }
        ))
    }
}
