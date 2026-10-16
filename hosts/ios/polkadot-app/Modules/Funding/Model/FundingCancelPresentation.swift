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

    /// Whether the user can cancel a session in `status`. Before money moved a
    /// second cancel ends the session, so a provider that went silent cannot hold
    /// it; after, a cancel already with the provider waits for its answer.
    static func canCancel(_ status: FundingStatus, moneyMoved: Bool) -> Bool {
        switch status {
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
    static func cancelTitle(_ status: FundingStatus) -> String {
        if case .cancelRequested = status {
            String(localized: .Funding.cancelAnyway)
        } else {
            String(localized: .Funding.depositCancel)
        }
    }
}

extension FundingSession {
    /// Whether money moved for the session: the provider started or named funds,
    /// or saw the user's payment. The core's rule is the same; from then on only
    /// the provider ends the session.
    var moneyMoved: Bool {
        !started.isEmpty || updates.contains { $0.update.movesMoney }
    }
}

private extension FundingUpdate {
    var movesMoney: Bool {
        switch self {
        case .paymentReceived,
             .crediting,
             .collecting,
             .refunding:
            true
        default:
            false
        }
    }
}
