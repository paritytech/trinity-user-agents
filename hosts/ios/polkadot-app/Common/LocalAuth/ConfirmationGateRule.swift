import Foundation
@preconcurrency import SubstrateSdk
import ChainRegistry

protocol ConfirmationGateRuleProtocol: Sendable {
    func requiresAuthentication(amount: Balance, total: Balance?) -> Bool
}

/// Decides whether a payment needs device authentication. Unknown total fails closed.
struct ConfirmationGateRule: ConfirmationGateRuleProtocol {
    let absoluteLimit: Balance
    let shareOfTotal: BigRational

    func requiresAuthentication(amount: Balance, total: Balance?) -> Bool {
        guard let total else { return true }

        return amount > min(absoluteLimit, shareOfTotal.mul(value: total))
    }
}

extension ConfirmationGateRule {
    /// The app limits in planks of the asset. A limit that cannot be converted is zero, so every payment asks.
    init(asset: AssetModel) {
        let absoluteLimit = AppConfig.ConfirmationLimit.absolute
            .toSubstrateAmount(precision: asset.decimalPrecision) ?? 0

        self.init(absoluteLimit: absoluteLimit, shareOfTotal: AppConfig.ConfirmationLimit.shareOfTotal)
    }
}
