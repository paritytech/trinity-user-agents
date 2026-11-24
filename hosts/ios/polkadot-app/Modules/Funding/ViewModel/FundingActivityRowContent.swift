import Foundation
import TrUAPIHost

/// What one activity row says, worked out apart from its view.
struct FundingActivityRowContent: Equatable {
    enum Icon: Equatable {
        case pending(delayed: Bool)
        case received
        case sent
        case failed
    }

    enum Tone: Equatable {
        case success
        case primary
        case secondary
    }

    let icon: Icon
    /// The amount: unsigned in flight and once nothing moved, signed once
    /// value moved.
    let title: String
    let titleTone: Tone
    let subtitle: String
    let isSubtitleWarning: Bool
    /// The rail, in the pill on the right.
    let pill: String?

    /// `moment` is when the session ended, as the list words it.
    init(item: FundingActivityItem, title: String, moment: String) {
        let isDelayed = item.isDelayed
        icon = Self.icon(item, isDelayed: isDelayed)
        self.title = title
        titleTone = Self.tone(item)
        subtitle = Self.subtitle(item, moment: moment)
        isSubtitleWarning = isDelayed
        pill = item.rail?.title
    }
}

private extension FundingActivityRowContent {
    static func icon(_ item: FundingActivityItem, isDelayed: Bool) -> Icon {
        switch item.status {
        case .inProgress: .pending(delayed: isDelayed)
        case .toppedUp: .received
        case .sent: .sent
        case .refunded,
             .payoutFailed,
             .cancelled,
             .failed: .failed
        }
    }

    static func tone(_ item: FundingActivityItem) -> Tone {
        switch item.status {
        case .toppedUp: .success
        case .sent,
             .inProgress: .primary
        case .refunded,
             .payoutFailed,
             .cancelled,
             .failed: .secondary
        }
    }

    static func subtitle(_ item: FundingActivityItem, moment: String) -> String {
        switch item.status {
        case let .inProgress(status):
            FundingWording.inFlight(status, direction: item.direction) ?? moment
        case .toppedUp,
             .sent:
            moment
        case .refunded:
            String(localized: .Funding.activityRefundedAt(moment: moment))
        case .payoutFailed:
            String(localized: .Funding.activityPayoutFailedAt(moment: moment))
        case .cancelled:
            String(localized: .Funding.activityCancelledAt(moment: moment))
        case .failed(.expired):
            String(localized: .Funding.activityExpiredAt(moment: moment))
        case .failed:
            String(localized: .Funding.activityPaymentFailedAt(moment: moment))
        }
    }
}
