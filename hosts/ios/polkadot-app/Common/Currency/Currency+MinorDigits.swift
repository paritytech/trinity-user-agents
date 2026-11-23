import Foundation

extension Currency {
    /// The digits after the separator its amounts are written with: 2 for
    /// EUR, 0 for JPY, 3 for KWD.
    var minorDigits: UInt16 {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.currencyCode = code
        return UInt16(clamping: formatter.maximumFractionDigits)
    }
}
