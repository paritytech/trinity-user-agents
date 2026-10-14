import Foundation

/// How much of the policy a caller can afford to wait for.
///
/// The balance renders nothing until the first verdicts land, so the pass that produces them cannot be
/// held up by a chain read a lagging node may take seconds to answer.
public enum BalanceEvaluationMode: Sendable {
    /// Every limit is consulted, however long the chain takes to answer.
    case complete

    /// Only the limits already in hand are applied. Whatever a slower limit would have gated stays
    /// spendable until the ``complete`` pass that follows says otherwise, so this trades a downward
    /// correction moments later for a balance the user can see at once.
    case immediate
}
