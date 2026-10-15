import Foundation
import TrUAPIHost

/// What the app shows for a user's cancel: after asking, after answering the
/// provider's offer, and while the cancel is with the provider.
enum FundingCancelPresentation {
    /// What follows a cancel: the overlay closes when the session ended,
    /// otherwise a toast says where the cancel stands.
    enum AfterCancel: Equatable {
        case close
        case toast(String)
    }

    static func after(_ outcome: CancelOutcome) -> AfterCancel {
        switch outcome {
        case .ended: .close
        case .requested,
             .pending: .toast(String(localized: .Funding.activityCancelRequested))
        case .tooLate: .toast(String(localized: .Funding.cancelTooLate))
        case .refused: .toast(String(localized: .Funding.errorCancelRefused))
        }
    }

    /// The toast after answering the provider's offer; none when the user
    /// kept going.
    static func after(_ answer: CancelConfirmation) -> String? {
        switch answer {
        case .accepted: String(localized: .Funding.offerAccepted)
        case .declined: nil
        case .lapsed: String(localized: .Funding.offerLapsed)
        case .noOffer: String(localized: .Funding.offerNone)
        }
    }

    /// The line for a session whose cancel is with the provider, and whether
    /// it reads as a warning. `compact` leaves out a refusal's reason, for a
    /// list row.
    static func line(for status: FundingStatus, compact: Bool = false) -> (text: String, isWarning: Bool)? {
        switch status {
        case .cancelRequested:
            (String(localized: .Funding.activityCancelRequested), false)
        case .cancelOffered:
            (String(localized: .Funding.activityCancelOffered), true)
        case let .cancelRefused(reason):
            if compact {
                (String(localized: .Funding.activityCancelRefused), true)
            } else {
                (String(localized: .Funding.cancelRefusedNote(reason: reason)), true)
            }
        case .cancelling:
            (String(localized: .Funding.activityCancelling), false)
        default:
            nil
        }
    }

    /// Whether the cancel is already with the provider, so the user cannot
    /// ask again.
    static func isUnderway(_ status: FundingStatus) -> Bool {
        switch status {
        case .cancelRequested,
             .cancelOffered,
             .cancelling:
            true
        default:
            false
        }
    }
}
