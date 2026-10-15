import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The CASH card's funding lists: what is in progress, on one card when there is
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
                            .contentShape(Rectangle())
                            .onTapGesture { center.showEnded(intent: item.id) }
                    }
                }
            }
        }
        .task { await follow() }
        .onDisappear { isStackExpanded = false }
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

    var isStacked: Bool {
        center.inFlight.count > 1
    }

    /// The newest session on one card, with how many are in flight on its
    /// icon; with more than one, a tap opens them all in a panel.
    @ViewBuilder
    var inProgress: some View {
        if isStacked, isStackExpanded {
            expandedPanel
        } else if let newest = center.inFlight.first {
            FundingActivityRow(
                item: newest,
                cash: cash,
                showsDivider: false,
                stackCount: isStacked ? center.inFlight.count : nil
            )
            .padding(.horizontal, DSSpacings.medium)
            .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
            .contentShape(RoundedRectangle(cornerRadius: DSRadii.large))
            .onTapGesture {
                if isStacked {
                    withAnimation(.spring) { isStackExpanded = true }
                } else {
                    center.open(intent: newest.id)
                }
            }
        }
    }

    var expandedPanel: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.spring) { isStackExpanded = false }
            } label: {
                HStack {
                    Text(.Funding.activityInProgress)
                        .typography(.titleSmall)
                        .foregroundStyle(.fgPrimary)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.fgPrimary)
                }
                .padding(.vertical, DSSpacings.smallIncreased)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            ScrollView {
                VStack(spacing: 0) {
                    ForEach(center.inFlight) { item in
                        FundingActivityRow(item: item, cash: cash, showsDivider: item.id != center.inFlight.last?.id)
                            .contentShape(Rectangle())
                            .onTapGesture { center.open(intent: item.id) }
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(maxHeight: Self.panelRowsHeight)
        }
        .padding(.horizontal, DSSpacings.medium)
        .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
    }

    /// Room for about seven sessions before the panel scrolls.
    static let panelRowsHeight: CGFloat = 588

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
    /// How many sessions are in flight, drawn in the icon of the one on top.
    var stackCount: Int?

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
        guard case let .inProgress(status) = item.status else { return false }
        return status.isDelayed
    }

    @ViewBuilder
    var icon: some View {
        switch item.status {
        case .inProgress:
            Group {
                if let stackCount {
                    FundingCountRing(count: stackCount)
                } else {
                    ProgressView()
                        .tint(isAmber ? Color.fgWarning : Color.fgPrimary)
                }
            }
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
        switch item.status {
        case let .failed(failure):
            failure.failedText
        case let .payoutFailed(.some(refunded)):
            String(localized: .Funding.activityRefundedAmount(amount: cash.label(cash.decimal(refunded))))
        case let .inProgress(status):
            inProgressText(status) ?? moment
        case .toppedUp,
             .sent,
             .refunded,
             .payoutFailed:
            moment
        }
    }

    var moment: String {
        let when = FundingActivityDate.moment(item.date)
        guard let rail = item.rail else { return when }
        return "\(rail.title) · \(when)"
    }

    func inProgressText(_ status: FundingStatus) -> String? {
        waitingText(status) ?? movingText(status)
    }

    /// The line for a session waiting on the user or the provider.
    func waitingText(_ status: FundingStatus) -> String? {
        switch status {
        case .waitingForPayment: String(localized: .Funding.activityWaitingPayment)
        case .waitingForTransfer: String(localized: .Funding.activityWaitingTransfer)
        case .waitingForDeposit: String(localized: .Funding.activityWaitingDeposit)
        case .retrying: String(localized: .Funding.activityRetrying)
        case .retryingPayout: String(localized: .Funding.activityRetryingPayout)
        case .takingLonger: String(localized: .Funding.activityDelayed)
        default: nil
        }
    }

    /// The line for a session whose value is on its way.
    func movingText(_ status: FundingStatus) -> String? {
        switch status {
        case .convertingToCash: String(localized: .Funding.activityConverting(symbol: cash.symbol))
        case .transactionInitiated: String(localized: .Funding.activityInitiated)
        case .preparingPayout: String(localized: .Funding.activityPreparingPayout)
        case let .convertingTo(asset): String(localized: .Funding.activityConvertingOut(asset: asset))
        default: nil
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

/// How many sessions are in flight, inside a turning ring.
private struct FundingCountRing: View {
    let count: Int

    @State private var isTurning = false

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color.fgTertiary, lineWidth: 2)
            Circle()
                .trim(from: 0, to: 0.25)
                .stroke(Color.fgPrimary, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(isTurning ? 360 : 0))
            Text(verbatim: "\(count)")
                .typography(.titleSmall)
                .foregroundStyle(.fgPrimary)
        }
        .padding(DSSpacings.small)
        .onAppear {
            withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) { isTurning = true }
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
