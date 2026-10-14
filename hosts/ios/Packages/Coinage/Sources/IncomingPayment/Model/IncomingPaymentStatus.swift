import Foundation
import SubstrateSdk

/// Observable status of an incoming payment. The live statuses are derived from the CoinageTx
/// durability layer plus on-chain detection and never persisted; only the terminal verdict is
/// written to the record, as `IncomingPaymentTerminalOutcome`, and read back exactly.
///
/// Terminal statuses never advance: `claimed(finalized: true)`, `claimedPartially`, `notClaimed`.
public enum IncomingPaymentStatus: Equatable, Sendable {
    /// Waiting for the needed amount to appear on the source.
    case detecting
    /// Claim in progress.
    case claiming
    /// At least `amount` was claimed at best/finalized head, depending on `finalized`.
    /// Terminal when `finalized == true`.
    case claimed(finalized: Bool)
    /// Terminal. Only `actualClaimed < amount` could be claimed.
    case claimedPartially(actualClaimed: Balance)
    /// Terminal. Nothing could be claimed.
    case notClaimed
}

public extension IncomingPaymentStatus {
    var isTerminal: Bool {
        switch self {
        case .detecting,
             .claiming:
            false
        case let .claimed(finalized):
            finalized
        case .claimedPartially,
             .notClaimed:
            true
        }
    }

    /// The status a persisted terminal verdict reports — read back exactly, never re-derived.
    init(outcome: IncomingPaymentTerminalOutcome) {
        switch outcome {
        case .claimed:
            self = .claimed(finalized: true)
        case let .claimedPartially(actualClaimed):
            self = .claimedPartially(actualClaimed: actualClaimed)
        case .notClaimed:
            self = .notClaimed
        }
    }

    /// The persistable verdict for a terminal status, or `nil` while nothing is settled. A
    /// non-finalized `claimed` is not terminal and yields `nil`.
    var terminalOutcome: IncomingPaymentTerminalOutcome? {
        switch self {
        case let .claimed(finalized):
            finalized ? .claimed : nil
        case let .claimedPartially(actualClaimed):
            .claimedPartially(actualClaimed: actualClaimed)
        case .notClaimed:
            .notClaimed
        case .detecting,
             .claiming:
            nil
        }
    }
}
