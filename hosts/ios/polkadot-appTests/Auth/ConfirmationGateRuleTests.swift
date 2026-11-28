import Foundation
import Testing
import SubstrateSdk
import ChainRegistry
@testable import polkadot_app

@Suite("ConfirmationGateRule")
struct ConfirmationGateRuleTests {
    @Test(
        "Asks above the absolute limit or above the share of the total, and when the total is unknown",
        arguments: [
            (amount: Balance(50), total: Balance?(1_000), expected: false),
            (amount: 51, total: 1_000, expected: true),
            (amount: 20, total: 40, expected: false),
            (amount: 21, total: 40, expected: true),
            (amount: 1, total: nil, expected: true)
        ]
    )
    func requiresAuthentication(amount: Balance, total: Balance?, expected: Bool) {
        let rule = ConfirmationGateRule(absoluteLimit: 50, shareOfTotal: .percent(of: 50))

        #expect(rule.requiresAuthentication(amount: amount, total: total) == expected)
    }

    @Test(
        "Converts 50 Cash to planks of the asset",
        arguments: [
            (amount: Balance(500_000_000_000), expected: false),
            (amount: 500_000_000_001, expected: true)
        ]
    )
    func appLimits(amount: Balance, expected: Bool) {
        let tenDecimalAsset = AssetModel(remoteModel: ChainMock.makeRemoteAsset(), enabled: true)
        let rule = ConfirmationGateRule(asset: tenDecimalAsset)

        #expect(rule.requiresAuthentication(amount: amount, total: 10_000_000_000_000) == expected)
    }
}
