import Foundation
import Coinage
import SubstrateSdk

/// Decides whether a product payment needs device authentication, from the user's total Cash.
protocol PaymentConfirmationGating: Sendable {
    func requiresAuthentication(amount: Balance) async -> Bool
}

final class PaymentConfirmationGate: PaymentConfirmationGating {
    private let coinageService: CoinageServicing?
    private let rule: ConfirmationGateRuleProtocol

    init(coinageService: CoinageServicing?, rule: ConfirmationGateRuleProtocol) {
        self.coinageService = coinageService
        self.rule = rule
    }

    func requiresAuthentication(amount: Balance) async -> Bool {
        await rule.requiresAuthentication(amount: amount, total: total())
    }
}

private extension PaymentConfirmationGate {
    func total() async -> Balance? {
        guard let coinageService else { return nil }

        do {
            let balanceService = try await coinageService.coinageBalanceService()
            for try await balance in balanceService.balanceStream.prefix(1) {
                return balance.total
            }
            return nil
        } catch {
            return nil
        }
    }
}
