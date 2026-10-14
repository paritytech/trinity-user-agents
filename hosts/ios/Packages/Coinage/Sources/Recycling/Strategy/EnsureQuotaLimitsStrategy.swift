import Foundation

/// The quota safety valve. When free-unload quota has crossed its reserve, this returns an empty
/// verdict map — the ``EnsureChainLimitsStrategy`` above then fills every unmapped coin with
/// `.allowUse` except the forced ones, i.e. "only recycle coins that must."
///
/// Quota is never surfaced to the user (it cannot be topped up or paid for), so it is auto-managed
/// here rather than exposed as a strategy axis.
public struct EnsureQuotaLimitsStrategy: CoinRecyclingStrategyProtocol {
    private let inner: CoinRecyclingStrategyProtocol
    private let quotaExhausted: Bool

    public init(inner: CoinRecyclingStrategyProtocol, quotaExhausted: Bool) {
        self.inner = inner
        self.quotaExhausted = quotaExhausted
    }

    public func evaluate(
        coins: [Coin],
        snapshot: RecyclingSnapshot,
        context: DenominationBreakdownContext
    ) -> RecyclingVerdicts {
        quotaExhausted ? [:] : inner.evaluate(coins: coins, snapshot: snapshot, context: context)
    }

    public func isVoucherUsable(_ voucher: Voucher, context: VoucherUsabilityContext) -> Bool {
        inner.isVoucherUsable(voucher, context: context)
    }

    public func allowsConfirmedSpend() -> Bool {
        inner.allowsConfirmedSpend()
    }
}
