import Foundation
import SubstrateSdk

extension Balance {
    func decimal(assetInfo: AssetBalanceDisplayInfo) -> Decimal {
        Decimal.fromSubstrateAmount(
            self,
            precision: assetInfo.assetPrecision
        ) ?? 0
    }
}
