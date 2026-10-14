import Foundation
import BigInt

extension BigUInt? {
    func decimalOrZero(precision: UInt16) -> Decimal {
        guard let self, self != 0 else {
            return 0
        }
        return self.decimal(precision: precision)
    }
}
