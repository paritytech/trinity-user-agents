import Foundation
import SubstrateSdk

extension AppConfig {
    /// Largest Cash amount a payment may move without device authentication.
    enum ConfirmationLimit {
        static let absolute: Decimal = 50
        static let shareOfTotal = BigRational.percent(of: 50)
    }
}
