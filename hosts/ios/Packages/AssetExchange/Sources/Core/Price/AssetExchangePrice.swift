import Foundation
import SubstrateSdk

public typealias AssetExchangePrice = Decimal // rate to usd

public protocol AssetExchangePriceStoring {
    func getCurrencyId() -> Int?
    func fetchPrice(for chainAssetId: ChainAssetId) -> AssetExchangePrice?
}
