import Foundation
import Operation_iOS
import SubstrateSdk

protocol HydraPoolTokensFactoryProtocol {
    func availableDirections() -> CompoundOperationWrapper<[ChainAssetId: Set<ChainAssetId>]>

    func availableDirectionsForAsset(
        _ chainAssetId: ChainAssetId
    ) -> CompoundOperationWrapper<Set<ChainAssetId>>
}
