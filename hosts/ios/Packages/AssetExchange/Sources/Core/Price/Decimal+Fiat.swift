import Foundation
import BigInt

extension Decimal {
    static func fiatValue(
        from balance: BigUInt?,
        price: AssetExchangePrice?,
        precision: Int16
    ) -> Decimal {
        guard let balance, let rate = price else {
            return 0
        }

        let decimalBalance = Decimal.fromSubstrateAmount(
            balance,
            precision: precision
        ) ?? 0

        return decimalBalance * rate
    }
}
