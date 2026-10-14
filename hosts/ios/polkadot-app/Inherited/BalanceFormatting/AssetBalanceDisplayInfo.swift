import Foundation
import Foundation_iOS

struct AssetBalanceDisplayInfo: Hashable {
    let displayPrecision: UInt16
    let assetPrecision: Int16
    let symbol: String
    let symbolValueSeparator: String
    let symbolPosition: TokenSymbolPosition
    let icon: URL?
}
