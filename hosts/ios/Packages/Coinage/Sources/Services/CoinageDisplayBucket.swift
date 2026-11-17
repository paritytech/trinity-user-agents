import Foundation
import SubstrateSdk

/// The two buckets the app shows: Ready and Clearing.
///
/// Clearing is everything that is not spendable right now, whatever the reason: money in a recycler
/// earning privacy, and money still arriving. The user does not need to know which.
public enum CoinageDisplayBucket: Equatable, Sendable, CaseIterable {
    /// Spendable now, at no privacy cost.
    case ready
    /// Not spendable now, for any reason.
    case clearing
}

public extension CoinageAvailability {
    var displayBucket: CoinageDisplayBucket {
        switch self {
        case .availableNow: .ready
        case .notReady: .clearing
        }
    }
}

public extension CoinageBalance {
    /// Spendable now. The same figure as ``availablePrivate``, named for what the user is shown.
    var ready: Balance { availablePrivate }

    /// Everything else. Together with ``ready`` this is exactly ``total``.
    var clearing: Balance { gainingPrivacy.amount + pending }
}
