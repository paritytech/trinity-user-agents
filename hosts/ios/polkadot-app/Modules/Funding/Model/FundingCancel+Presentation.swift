import Foundation
import TrUAPIHost

// What the app shows for a user's cancel: after asking, after answering the
// provider's offer, and while the cancel is with the provider.

extension CancelOutcome {
    /// What follows a cancel: the overlay closes when the session ended,
    /// otherwise a toast says where the cancel stands.
    enum FollowUp: Equatable {
        case close
        case toast(String)
    }

    var followUp: FollowUp {
        switch self {
        case .ended: .close
        case .requested,
             .pending: .toast(String(localized: .Funding.activityCancelRequested))
        case .tooLate: .toast(String(localized: .Funding.cancelTooLate))
        case .refused: .toast(String(localized: .Funding.errorCancelRefused))
        }
    }
}

extension CancelConfirmation {
    /// The toast after answering the provider's offer; none when the user
    /// kept going.
    var toast: String? {
        switch self {
        case .accepted: String(localized: .Funding.offerAccepted)
        case .declined: nil
        case .lapsed: String(localized: .Funding.offerLapsed)
        case .noOffer: String(localized: .Funding.offerNone)
        case .tooLate: String(localized: .Funding.offerTooLate)
        }
    }
}

extension FundingStatus {
    /// Whether a withdrawal whose cancel the user accepted was paid out
    /// anyway: the payout landed before the provider could stop it.
    var cancelCameTooLate: Bool {
        if case .paidOutAfterCancel = self { return true }
        return false
    }

    /// The line for a session whose cancel is with the provider, and whether
    /// it reads as a warning. `compact` leaves out a refusal's reason, for a
    /// list row.
    func cancelLine(
        direction: FundingDirection = .in,
        compact: Bool = false
    ) -> (text: String, isWarning: Bool)? {
        switch self {
        case .cancelRequested:
            (String(localized: .Funding.activityCancelRequested), false)
        case .cancelOffered:
            (String(localized: .Funding.activityCancelOffered), true)
        case let .cancelRefused(reason):
            if compact {
                (String(localized: .Funding.activityCancelRefused), true)
            } else {
                direction == .out
                    ? (String(localized: .Funding.cancelRefusedWithdrawal(reason: reason)), true)
                    : (String(localized: .Funding.cancelRefusedTopUp(reason: reason)), true)
            }
        case .cancelling:
            (String(localized: .Funding.activityCancelling), false)
        default:
            nil
        }
    }

    /// Whether the user can cancel a session in this status, given the core's
    /// `FundingProgress.moneyMoved`. Before money moved a second cancel ends the
    /// session, so a provider that went silent cannot hold it; after, a cancel
    /// already with the provider waits for its answer.
    func canCancel(moneyMoved: Bool) -> Bool {
        switch self {
        case .cancelRequested: !moneyMoved
        case .cancelOffered,
             .cancelling:
            false
        default:
            true
        }
    }

    /// The cancel action's title: a second cancel, which ends the session at
    /// once, reads differently from the first.
    var cancelTitle: String {
        if case .cancelRequested = self {
            String(localized: .Funding.cancelAnyway)
        } else {
            String(localized: .Funding.depositCancel)
        }
    }
}
