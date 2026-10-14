import UIKit
import SubstrateSdk
import PolkadotUI

enum SelectTokenCellViewModel: Hashable {
    case chainAsset(ChainAssetViewModel)
    case fiat

    struct ChainAssetViewModel: Hashable {
        let chainAssetId: ChainAssetId
        let name: String
        let symbol: String
        let icon: ImageViewModelProtocol?

        func hash(into hasher: inout Hasher) {
            hasher.combine(chainAssetId)
            hasher.combine(name)
            hasher.combine(symbol)
        }

        static func == (lhs: Self, rhs: Self) -> Bool {
            guard
                (lhs.icon == nil && rhs.icon == nil) ||
                (lhs.icon != nil && rhs.icon != nil)
            else {
                return false
            }

            return lhs.hashValue == rhs.hashValue
        }
    }
}
