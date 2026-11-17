import Foundation
import SubstrateSdk

/// How far a claim of coins a peer handed us, or a top-up, has got. Derived from the operation's status
/// in the core; nothing here is persisted.
public enum CoinageTransferDetection: Equatable, Sendable {
    /// Nothing the keys control is on chain yet, so there is nothing to claim.
    case detecting

    /// Claims are under way and none of the coins is ours yet. No failures so far.
    case claiming

    /// `claimed` is ours; the coins still missing are being retried because an attempt failed. Only
    /// reported when something actually went wrong — a payment merely arriving in pieces is `claiming`.
    case claimingRest(claimed: Balance)

    /// Every coin is ours. `finalized` is false while the claims are only in a best-chain block;
    /// when true this state is terminal, otherwise it can be downgraded again.
    case claimed(amount: Balance, finalized: Bool)

    /// Claiming is over and `claimed` is all that will ever arrive. Only ever the last word.
    case claimedPartially(claimed: Balance)

    /// Nothing was claimed and nothing more will be tried. Only ever the last word.
    case notClaimed
}
