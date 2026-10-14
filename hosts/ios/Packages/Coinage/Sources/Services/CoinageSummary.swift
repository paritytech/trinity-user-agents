import Foundation

/// The balance and the individual holdings behind it, from one computation.
///
/// The pair travels together on purpose. Emitting them on separate streams let a display read the
/// figures from one evaluation and the holdings from the previous one, since the two subscriptions
/// resume independently — briefly showing totals that its own rows did not add up to. Combining two
/// streams downstream does not fix that either: `combineLatest` emits the mixed pair on the way to
/// the matched one.
public struct CoinageSummary: Equatable {
    public let balance: CoinageBalance
    public let holdings: CoinageHoldings

    public init(balance: CoinageBalance, holdings: CoinageHoldings) {
        self.balance = balance
        self.holdings = holdings
    }

    public static let empty = CoinageSummary(balance: .empty, holdings: .empty)
}
