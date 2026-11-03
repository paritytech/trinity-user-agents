import Foundation

extension FundingCash {
    /// The app's payment asset.
    @MainActor static var current: FundingCash {
        FundingCash(
            symbol: PaymentAssetBranding.shared.current.symbol,
            precision: PaymentRequestViewFactory.mainChainAsset()?.asset.decimalPrecision ?? 6
        )
    }
}
