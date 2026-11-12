import Foundation
import SubstrateSdk

/// The two buckets the app shows: Ready and Clearing.
///
/// The domain classifies a holding three ways — ``CoinageAvailability`` — but the user is only ever
/// shown two. Clearing is everything that is not spendable right now, whatever the reason: money in
/// a recycler earning privacy, money the chain will no longer accept until it is recycled, and
/// money still arriving. The user does not need to know which, and telling them would put a third
/// state on screen that was deliberately removed.
///
/// A holding already in a recycler that the strategy considers fungible enough is ``ready``, not
/// ``clearing``. It keeps whatever wear it has earned, which falls as its ring fills.
///
/// This is the single place that collapse happens. Before it existed the same decision was made
/// three times over — once for the figures, once for the summary bar, once for the coins — and the
/// three did not agree: two dropped ``CoinageAvailability/pending`` from the display entirely while
/// the third folded it into Clearing, so the figures did not add up to the total and the bar's
/// white share could grow at the moment a coin became permanently unspendable.
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
        // Earning privacy, forced to recycle, or still arriving: all of it is simply not ready.
        case .gainingPrivacy,
             .pending: .clearing
        }
    }
}

public extension CoinageBalance {
    /// Spendable now. The same figure as ``availablePrivate``, named for what the user is shown.
    var ready: Balance { availablePrivate }

    /// Everything else. Together with ``ready`` this is exactly ``total``, which is the invariant
    /// the two displayed figures have to keep against the headline above them.
    var clearing: Balance { gainingPrivacy.amount + pending }
}
