import Foundation

/// The recycling policy seam: one parametric implementation, wrapped by two chain-fact decorators.
///
/// `evaluate` is pure and synchronous — coin value comes from the already-resolved, cached
/// ``DenominationBreakdownContext`` (`valueInPlanks(for:)` = `unit << exponent`), so no async work
/// happens here. Only the evaluator's quota and ring-capacity reads are async, off the throttled path.
public protocol CoinRecyclingStrategyProtocol {
    /// Gate verdict per coin. Batched so the budget is spent once across the whole active set.
    func evaluate(
        coins: [Coin],
        snapshot: RecyclingSnapshot,
        context: DenominationBreakdownContext
    ) -> RecyclingVerdicts

    /// Whether an in-recycler voucher counts as usable under this strategy.
    func isVoucherUsable(_ voucher: Voucher, context: VoucherUsabilityContext) -> Bool

    /// Whether gaining-privacy balance may be spent behind a confirmation.
    func allowsConfirmedSpend() -> Bool
}
