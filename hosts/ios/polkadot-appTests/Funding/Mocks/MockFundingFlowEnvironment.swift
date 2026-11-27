import SubstrateSdk
@testable import polkadot_app

/// The app as a funding flow sees it: the payment balance's asset and what a
/// payment can spend, which the test sets.
final class MockFundingFlowEnvironment: FundingFlowEnvironment {
    let cashInfo: AssetBalanceDisplayInfo = .digitalDollarFigure(assetPrecision: 6)
    let cashSymbol = "CASH"
    let branding: FundingProviderBranding = MockFundingBranding()
    let iconLoader: FundingIconLoader? = nil
    /// What a payment can spend; nil when the balance cannot be followed.
    var spendable: Balance?

    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void) async {
        guard let spendable else { return }
        onChange(spendable)
    }
}
