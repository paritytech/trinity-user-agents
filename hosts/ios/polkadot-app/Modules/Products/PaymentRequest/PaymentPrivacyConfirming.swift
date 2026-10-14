import Foundation
import SubstrateSdk

/// Asks the user to confirm a product payment that dips into gaining-privacy funds.
protocol PaymentPrivacyConfirming: Sendable {
    func confirmGainingPrivacySpend(amount: Balance) async -> Bool
}
