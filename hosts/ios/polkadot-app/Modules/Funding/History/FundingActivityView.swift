import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The CASH card's funding lists: what is in progress, stacked when there is
/// more than one, then the history by day.
struct FundingActivityView: View {
    let cash: FundingCash
    var center: FundingActivityCenter = .shared

    @State private var isStackExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.medium) {
            if !center.inFlight.isEmpty {
                inProgress
            }
            ForEach(days, id: \.title) { day in
                VStack(alignment: .leading, spacing: 0) {
                    Text(verbatim: day.title)
                        .typography(.titleSmall)
                        .foregroundStyle(.fgPrimary)
                        .padding(.bottom, DSSpacings.small)
                    ForEach(day.items) { item in
                        FundingActivityRow(item: item, cash: cash)
                    }
                }
            }
        }
        .task { await follow() }
    }
}

private extension FundingActivityView {
    struct Day {
        let title: String
        let items: [FundingActivityItem]
    }

    var days: [Day] {
        var result: [Day] = []
        for item in center.history {
            let title = FundingActivityDate.day(item.date)
            if let last = result.last, last.title == title {
                result[result.count - 1] = Day(title: title, items: last.items + [item])
            } else {
                result.append(Day(title: title, items: [item]))
            }
        }
        return result
    }

    var inProgress: some View {
        VStack(alignment: .leading, spacing: DSSpacings.small) {
            HStack(spacing: DSSpacings.small) {
                Text(.Funding.activityInProgress)
                    .typography(.titleSmall)
                    .foregroundStyle(.fgPrimary)
                Text(verbatim: "\(center.inFlight.count)")
                    .typography(.labelSmallEmphasized)
                    .foregroundStyle(.fgPrimary)
                    .padding(.horizontal, 7)
                    .frame(height: 20)
                    .background(.bgSurfaceNested, in: Capsule())
            }

            if center.inFlight.count > 1, !isStackExpanded {
                stacked
            } else {
                ForEach(center.inFlight) { item in
                    card(item)
                }
            }
        }
    }

    /// The first session on top of the edges of the ones behind it.
    var stacked: some View {
        ZStack(alignment: .top) {
            ForEach(Array(center.inFlight.prefix(3).enumerated().reversed()), id: \.element.id) { index, item in
                card(item)
                    .scaleEffect(1 - CGFloat(index) * 0.04, anchor: .bottom)
                    .offset(y: CGFloat(index) * 10)
                    .opacity(index == 0 ? 1 : 0.6)
            }
        }
        .padding(.bottom, CGFloat(min(center.inFlight.count, 3) - 1) * 10)
        .onTapGesture { withAnimation(.spring) { isStackExpanded = true } }
    }

    func card(_ item: FundingActivityItem) -> some View {
        FundingActivityRow(item: item, cash: cash, showsDivider: false)
            .padding(.horizontal, DSSpacings.medium)
            .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
    }

    /// Refreshes while the card is open, so a step reached without a status
    /// change still shows.
    func follow() async {
        while !Task.isCancelled {
            center.refresh()
            try? await Task.sleep(for: .seconds(10))
        }
    }
}

struct FundingActivityRow: View {
    let item: FundingActivityItem
    let cash: FundingCash
    var showsDivider = true

    var body: some View {
        HStack(spacing: DSSpacings.smallIncreased) {
            icon
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                Text(subtitle)
                    .typography(.bodySmall)
                    .foregroundStyle(isAmber ? Color.fgWarning : Color.fgSecondary)
            }
            Spacer()
            Text(verbatim: amountText)
                .typography(.bodyLargeEmphasized)
                .foregroundStyle(amountColor)
        }
        .padding(.vertical, DSSpacings.smallIncreased)
        .overlay(alignment: .bottom) {
            if showsDivider {
                Divider().overlay(Color.strokePrimary).padding(.leading, 60)
            }
        }
    }
}

private extension FundingActivityRow {
    var isAmber: Bool {
        guard case let .inProgress(step, isDelayed) = item.status else { return false }
        return isDelayed || step == .retrying
    }

    @ViewBuilder
    var icon: some View {
        switch item.status {
        case .inProgress:
            ProgressView()
                .tint(isAmber ? Color.fgWarning : Color.fgPrimary)
                .frame(width: 48, height: 48)
                .background(isAmber ? Color.bgStatusWarning.opacity(0.3) : Color.bgSurfaceNested, in: Circle())
        case .toppedUp,
             .sent:
            Image(systemName: item.direction == .in ? "arrow.down.left" : "arrow.up.right")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(item.direction == .in ? Color.fgPrimaryInverted : Color.fgPrimary)
                .frame(width: 48, height: 48)
                .background(item.direction == .in ? Color.bgActionPrimary : Color.bgSurfaceNested, in: Circle())
        case .refunded,
             .payoutFailed,
             .failed:
            Image(systemName: "xmark")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.fgError)
                .frame(width: 48, height: 48)
                .background(Color.bgStatusError.opacity(0.25), in: Circle())
        }
    }

    var title: String {
        switch item.status {
        case .inProgress:
            if item.direction == .in {
                return String(localized: .Funding.activityToppingUp)
            }
            return String(localized: .Funding.activitySending)
        case .toppedUp: return String(localized: .Funding.activityToppedUp)
        case .sent: return String(localized: .Funding.activitySent)
        case .refunded: return String(localized: .Funding.activityRefunded)
        case .payoutFailed: return String(localized: .Funding.activityPayoutFailed)
        case .failed:
            if item.direction == .in {
                return String(localized: .Funding.activityTopUpFailed)
            }
            return String(localized: .Funding.activityWithdrawFailed)
        }
    }

    var subtitle: String {
        guard case let .inProgress(step, isDelayed) = item.status else {
            let when = FundingActivityDate.moment(item.date)
            guard let rail = item.rail else { return when }
            return "\(rail.title) · \(when)"
        }

        if isDelayed { return String(localized: .Funding.activityDelayed) }

        switch step {
        case .upcoming: return String(localized: .Funding.activityUpcoming)
        case .waitingForTransfer: return String(localized: .Funding.activityWaitingTransfer)
        case .converting: return String(localized: .Funding.activityConverting(symbol: cash.symbol))
        case .retrying: return String(localized: .Funding.activityRetrying)
        case .transactionInitiated: return String(localized: .Funding.activityInitiated)
        case let .convertingOut(asset): return String(localized: .Funding.activityConvertingOut(asset: asset))
        }
    }

    var amountText: String {
        guard let units = item.amount else { return "" }

        let value = cash.decimal(units)
        switch item.status {
        case .refunded,
             .payoutFailed,
             .failed:
            return cash.label(value)
        case .inProgress,
             .toppedUp,
             .sent:
            return cash.signed(value, direction: item.direction)
        }
    }

    var amountColor: Color {
        switch item.status {
        case .toppedUp: .fgSuccess
        case .sent: .fgPrimary
        case .inProgress,
             .refunded,
             .payoutFailed,
             .failed: .fgSecondary
        }
    }
}

/// The history's dates: "Today at 08:25", "Yesterday at 18:40",
/// "4 Oct at 16:20", and "4 Oct 2025" for an earlier year.
enum FundingActivityDate {
    static func moment(_ date: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        let time = date.formatted(date: .omitted, time: .shortened)
        if calendar.isDateInToday(date) {
            return String(localized: .Funding.dateTodayAt(time: time))
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: .Funding.dateYesterdayAt(time: time))
        }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return String(localized: .Funding.dateDayAt(day: dayMonth(date), time: time))
        }
        return date.formatted(.dateTime.day().month(.abbreviated).year())
    }

    /// The heading a day's rows sit under.
    static func day(_ date: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        if calendar.isDateInToday(date) { return String(localized: .Funding.dateToday) }
        if calendar.isDateInYesterday(date) { return String(localized: .Funding.dateYesterday) }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) { return dayMonth(date) }
        return date.formatted(.dateTime.day().month(.abbreviated).year())
    }

    private static func dayMonth(_ date: Date) -> String {
        date.formatted(.dateTime.day().month(.abbreviated))
    }
}
