import Foundation
import SubstrateSdk

enum DepositRateError: Error {
    case invalidBalances
}

extension Decimal {
    static func rateFromSubstrate(
        amount1: Balance,
        amount2: Balance,
        precision1: Int16,
        precision2: Int16
    ) throws -> Decimal {
        guard
            let decimal1 = fromSubstrateAmount(amount1, precision: precision1),
            let decimal2 = fromSubstrateAmount(amount2, precision: precision2),
            decimal2 > 0 else {
            throw DepositRateError.invalidBalances
        }

        return decimal2 / decimal1
    }
}
