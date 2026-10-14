import Foundation
import SubstrateSdk

/// What the user holds, split by whether they can spend it right now.
///
/// The middle bucket is the interesting one: money the strategy is deliberately holding back, which some
/// strategies will still release behind a confirmation. ``pending`` never will. Amounts are in planks —
/// use ``CoinageBalanceServiceProtocol/denominationContext`` to render them as decimals.
public struct CoinageBalance: Equatable {
    /// Spendable now at no privacy cost: coins the strategy leaves usable plus fully-usable vouchers.
    public let availablePrivate: Balance
    public let gainingPrivacy: GainingPrivacy
    /// On its way, or past the age the chain still accepts. Not spendable on any terms.
    public let pending: Balance

    public struct GainingPrivacy: Equatable {
        public let amount: Balance
        /// Whether the user may spend ``amount`` after confirming. The privacy earned so far is lost if
        /// they do, which is why it takes a confirmation instead of being part of `availablePrivate`.
        public let canSpendWithConfirmation: Bool

        public init(amount: Balance, canSpendWithConfirmation: Bool) {
            self.amount = amount
            self.canSpendWithConfirmation = canSpendWithConfirmation
        }
    }

    public init(availablePrivate: Balance, gainingPrivacy: GainingPrivacy, pending: Balance) {
        self.availablePrivate = availablePrivate
        self.gainingPrivacy = gainingPrivacy
        self.pending = pending
    }

    /// Everything the chosen strategy will let the user part with, including what it holds back but would
    /// release on confirmation. ``availablePrivate`` alone is the subset that costs no privacy to spend.
    public var available: Balance {
        gainingPrivacy.canSpendWithConfirmation ? availablePrivate + gainingPrivacy.amount : availablePrivate
    }

    public var total: Balance {
        availablePrivate + gainingPrivacy.amount + pending
    }

    public static let empty = CoinageBalance(
        availablePrivate: 0,
        gainingPrivacy: GainingPrivacy(amount: 0, canSpendWithConfirmation: true),
        pending: 0
    )
}
