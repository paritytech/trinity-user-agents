import Foundation

/// The per-coin gate verdict produced by a ``CoinRecyclingStrategyProtocol``. Derived on every
/// evaluation and never persisted.
///
/// The two "held back" cases are distinct because they answer "may the user spend it anyway?"
/// differently: ``mustRecycle`` is a chain fact and can never be offered, whereas ``toRecycle`` is
/// the policy's own choice and may be offered behind a confirmation.
public enum CoinRecyclingState: Equatable, Sendable {
    /// Past the age the chain still accepts. Counts as pending and is never offered for a spend —
    /// the chain would reject it.
    case mustRecycle

    /// The strategy chose to hold the coin back for privacy. Counts as gaining-privacy and may be
    /// offered behind a confirmation when the strategy allows it.
    case toRecycle

    /// Free to spend now.
    case allowUse
}

public extension CoinRecyclingState {
    /// Whether this verdict enqueues the coin for recycling. Both held-back verdicts do; only the
    /// balance bucketing distinguishes them.
    var triggersRecycling: Bool {
        switch self {
        case .mustRecycle,
             .toRecycle: true
        case .allowUse: false
        }
    }
}
