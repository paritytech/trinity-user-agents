import Foundation

/// Answers "which coins and vouchers may this spend draw on" for a given ``SpendScope``, applying the
/// recycling verdicts and the current strategy's voucher usability.
///
/// Two invariants hold by construction:
/// - `.mustRecycle` coins are never returned — no confirmation makes a chain-rejected coin spendable.
/// - `.withConfirmation` cannot override the strategy: it widens to gaining-privacy funds only when
///   the strategy allows confirmed spends, so under `maxPrivacy` it equals `.spendable`.
public struct CoinageAssetSelector {
    private let preClassificator: any CoinageAssetsPreClassificating

    public init(preClassificator: any CoinageAssetsPreClassificating) {
        self.preClassificator = preClassificator
    }

    public func selectableCoins(
        _ coins: [TrackedCoin],
        verdicts: RecyclingVerdicts,
        allowsConfirmedSpend: Bool,
        scope: SpendScope
    ) -> [TrackedCoin] {
        let widen = scope == .withConfirmation && allowsConfirmedSpend

        return preClassificator.preClassifyCoins(coins).minted.filter { tracked in
            switch verdicts[tracked.coin.derivationIndex] {
            case .allowUse: true
            case .toRecycle: widen
            case .mustRecycle,
                 .none: false
            }
        }
    }

    public func selectableVouchers(
        _ vouchers: [TrackedVoucher],
        strategy: any CoinRecyclingStrategyProtocol,
        context: VoucherUsabilityContext,
        scope: SpendScope
    ) -> [TrackedVoucher] {
        let buckets = preClassificator.preClassifyVouchers(vouchers, strategy: strategy, context: context)
        let widen = scope == .withConfirmation && strategy.allowsConfirmedSpend()

        return widen ? buckets.usable + buckets.gainingPrivacy : buckets.usable
    }
}
