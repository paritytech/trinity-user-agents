import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Top-ups and withdrawals on one screen: those in flight under In progress,
/// the ended ones under Completed, newest first. A row opens its session.
struct FundingHistoryView: View {
    let cash: FundingCash
    var center: FundingActivityCenter = .shared
    var onBack: () -> Void

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: String(localized: .Funding.historyTitle), onBack: onBack)

            if center.inFlight.isEmpty, center.history.isEmpty {
                Text(.Funding.historyEmpty)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .padding(.top, DSSpacings.large)
                Spacer(minLength: 0)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: DSSpacings.large) {
                        section(String(localized: .Funding.activityInProgress), items: center.inFlight)
                        section(String(localized: .Funding.activityCompleted), items: center.history)
                    }
                }
            }
        }
        .task { await follow() }
    }

    @ViewBuilder
    private func section(_ title: String, items: [FundingActivityItem]) -> some View {
        if !items.isEmpty {
            VStack(alignment: .leading, spacing: DSSpacings.small) {
                Text(verbatim: title)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                ForEach(items) { item in
                    FundingActivityRow(item: item, cash: cash, arrives: center.isArriving(item.id))
                        .contentShape(Rectangle())
                        .onTapGesture { center.openDetail(item) }
                }
            }
        }
    }

    /// Refreshes while the screen is open, so a step reached without a status
    /// change still shows.
    private func follow() async {
        while !Task.isCancelled {
            center.refresh()
            try? await Task.sleep(for: .seconds(10))
        }
    }
}

/// What is in flight in one direction, shown when Get or Send money out is
/// tapped while a session is still running: the sessions, a way to the
/// history, and a button to start another.
struct FundingPendingView: View {
    let direction: FundingDirection
    let cash: FundingCash
    var center: FundingActivityCenter = .shared
    var onHistory: () -> Void
    var onNew: () -> Void

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            HStack {
                Text(verbatim: title)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                Spacer()
                Button(action: onHistory) {
                    Image(systemName: "clock.arrow.circlepath")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(.fgPrimary)
                        .frame(width: 36, height: 36)
                        .background(.bgSurfaceNested, in: Circle())
                }
                .accessibilityLabel(Text(.Funding.historyOpen))
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(items) { item in
                        FundingActivityRow(item: item, cash: cash)
                            .contentShape(Rectangle())
                            .onTapGesture { center.openDetail(item) }
                    }
                }
            }

            FundingPrimaryButton(title: buttonTitle, action: onNew)
        }
    }

    private var items: [FundingActivityItem] {
        center.inFlight.filter { $0.direction == direction }
    }

    private var title: String {
        direction == .in ? String(localized: .Funding.activityToppingUp) : String(localized: .Funding.activitySending)
    }

    private var buttonTitle: String {
        direction == .in ? String(localized: .Funding.pendingNewTopUp) : String(localized: .Funding.pendingSendOut)
    }
}

/// One session in a list: its amount, what it is doing or when it ended, and
/// the rail it went by.
struct FundingActivityRow: View {
    let item: FundingActivityItem
    let cash: FundingCash
    /// Just finished: the row arrives and its icon plays the completion.
    var arrives = false

    private static let iconSize: CGFloat = 20

    var body: some View {
        let content = FundingActivityRowContent(item: item, cash: cash)
        HStack(alignment: .top, spacing: DSSpacings.small) {
            FundingActivityIcon(icon: content.icon, animatesCompletion: arrives)
                .frame(width: Self.iconSize, height: Self.iconSize)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: content.title)
                    .typography(.bodyMediumEmphasized)
                    .foregroundStyle(content.titleTone.color)
                    .lineLimit(1)
                Text(content.subtitle)
                    .typography(.bodySmall)
                    .foregroundStyle(content.isSubtitleWarning ? Color.fgWarning : Color.fgSecondary)
                    .lineLimit(1)
            }
            Spacer(minLength: DSSpacings.small)
            if let pill = content.pill {
                Text(verbatim: pill)
                    .typography(.labelSmall)
                    .foregroundStyle(.fgSecondary)
                    .padding(.horizontal, DSSpacings.small)
                    .padding(.vertical, 2)
                    .background(.bgSurfaceNested, in: Capsule())
            }
        }
        .padding(DSSpacings.medium)
        .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))
        .modifier(FundingArrival(isArriving: arrives))
    }
}

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

        var color: Color {
            switch self {
            case .success: .fgSuccess
            case .primary: .fgPrimary
            case .secondary: .fgSecondary
            }
        }
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

    init(item: FundingActivityItem, cash: FundingCash, now: Date = Date(), calendar: Calendar = .current) {
        let isDelayed = item.isDelayed
        icon = Self.icon(item, isDelayed: isDelayed)
        title = Self.title(item, cash: cash)
        titleTone = Self.tone(item)
        subtitle = Self.subtitle(item, now: now, calendar: calendar)
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

    static func title(_ item: FundingActivityItem, cash: FundingCash) -> String {
        guard let units = item.amount else { return "" }

        let value = cash.decimal(units)
        switch item.status {
        case .toppedUp,
             .sent:
            return cash.signed(value, direction: item.direction)
        case .inProgress where item.direction == .out:
            return cash.signed(value, direction: .out)
        case .inProgress,
             .refunded,
             .payoutFailed,
             .cancelled,
             .failed:
            return cash.label(value)
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

    static func subtitle(_ item: FundingActivityItem, now: Date, calendar: Calendar) -> String {
        let moment = FundingActivityDate.moment(item.date, now: now, calendar: calendar)
        switch item.status {
        case let .inProgress(status):
            return FundingWording.inFlight(status, direction: item.direction)
        case .toppedUp,
             .sent:
            return moment
        case .refunded:
            return String(localized: .Funding.activityRefundedAt(moment: moment))
        case .payoutFailed:
            return String(localized: .Funding.activityPayoutFailedAt(moment: moment))
        case .cancelled:
            return String(localized: .Funding.activityCancelledAt(moment: moment))
        case .failed(.expired):
            return String(localized: .Funding.activityExpiredAt(moment: moment))
        case .failed:
            return String(localized: .Funding.activityPaymentFailedAt(moment: moment))
        }
    }
}

/// A row's icon: a ring while in flight (amber when slow), an arrow once value
/// moved, a cross once it failed.
struct FundingActivityIcon: View {
    let icon: FundingActivityRowContent.Icon
    /// Plays the ring turning into what happened, for a row that has just
    /// arrived.
    var animatesCompletion = false

    var body: some View {
        switch icon {
        case let .pending(delayed):
            ProgressView()
                .controlSize(.mini)
                .tint(delayed ? Color.fgWarning : Color.fgSecondary)
        case .received:
            FundingCompletionGlyph(kind: .received, animated: animatesCompletion)
                .foregroundStyle(.fgSuccess)
        case .sent:
            FundingCompletionGlyph(kind: .sent, animated: animatesCompletion)
                .foregroundStyle(.fgSecondary)
        case .failed:
            FundingCompletionGlyph(kind: .failed, animated: animatesCompletion)
                .foregroundStyle(.fgError)
        }
    }
}

/// When a row's session ended: "Today at 12:45", "Yesterday at 12:45",
/// "Jun 12 at 16:40", and "Jun 12, 2025" in an earlier year.
enum FundingActivityDate {
    static func moment(_ date: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        let time = date.formatted(date: .omitted, time: .shortened)
        if calendar.isDate(date, inSameDayAs: now) {
            return String(localized: .Funding.dateTodayAt(time: time))
        }
        if isYesterday(date, now: now, calendar: calendar) {
            return String(localized: .Funding.dateYesterdayAt(time: time))
        }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return String(localized: .Funding.dateDayAt(day: format(date, "MMM d", calendar: calendar), time: time))
        }
        return format(date, "MMM d, yyyy", calendar: calendar)
    }

    private static func isYesterday(_ date: Date, now: Date, calendar: Calendar) -> Bool {
        guard let yesterday = calendar.date(byAdding: .day, value: -1, to: now) else { return false }
        return calendar.isDate(date, inSameDayAs: yesterday)
    }

    private static func format(_ date: Date, _ pattern: String, calendar: Calendar) -> String {
        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.locale = calendar.locale ?? .current
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = pattern
        return formatter.string(from: date)
    }
}
