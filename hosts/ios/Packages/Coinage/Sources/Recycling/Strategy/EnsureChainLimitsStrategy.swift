import Foundation

/// Enforces the chain age ceiling: any coin at `age >= forcedRecyclingAge` is forced to
/// ``CoinRecyclingState/mustRecycle`` regardless of the inner verdict, **bypassing the budget**.
/// This is what lets `minPrivacy` carry a zero budget without stranding old coins.
///
/// It is `.mustRecycle`, not `.toRecycle`, so a chain-forced coin is never offered for a confirmed
/// spend — the chain would reject it.
public struct EnsureChainLimitsStrategy: CoinRecyclingStrategyProtocol {
    private let inner: CoinRecyclingStrategyProtocol
    private let forcedRecyclingAge: Int16

    public init(inner: CoinRecyclingStrategyProtocol, forcedRecyclingAge: Int16) {
        self.inner = inner
        self.forcedRecyclingAge = forcedRecyclingAge
    }

    public func evaluate(
        coins: [Coin],
        snapshot: RecyclingSnapshot,
        context: DenominationBreakdownContext
    ) -> RecyclingVerdicts {
        let verdicts = inner.evaluate(coins: coins, snapshot: snapshot, context: context)

        var result = RecyclingVerdicts()
        for coin in coins {
            let forced = (coin.age ?? -1) >= forcedRecyclingAge
            result[coin.derivationIndex] = forced ? .mustRecycle : (verdicts[coin.derivationIndex] ?? .allowUse)
        }
        return result
    }

    public func isVoucherUsable(_ voucher: Voucher, context: VoucherUsabilityContext) -> Bool {
        inner.isVoucherUsable(voucher, context: context)
    }

    public func allowsConfirmedSpend() -> Bool {
        inner.allowsConfirmedSpend()
    }
}
