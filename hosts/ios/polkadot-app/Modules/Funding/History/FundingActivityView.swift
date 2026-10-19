import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The Purse's history, by day. What is in flight sits in
/// ``FundingInProgressDock`` instead.
struct FundingActivityView: View {
    let cash: FundingCash
    /// The balance now, which each day's closing balance is worked back from.
    var currentBalance: Decimal?
    var center: FundingActivityCenter = .shared

    var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.large) {
            ForEach(Array(days.enumerated()), id: \.element.title) { index, day in
                VStack(alignment: .leading, spacing: 0) {
                    Text(verbatim: day.title)
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(.fgPrimary)
                        .padding(.bottom, DSSpacings.small)
                        .background { dayMark(day, balance: closingBalances[safe: index]) }
                    ForEach(day.items) { item in
                        FundingActivityRow(item: item, cash: cash, showsDivider: item.id != day.items.last?.id)
                            .contentShape(Rectangle())
                            .onTapGesture { center.showEnded(intent: item.id) }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, center.inFlight.isEmpty ? 0 : FundingInProgressDock.reservedHeight)
        .task { await follow() }
    }
}

private extension FundingActivityView {
    struct Day {
        let title: String
        let isToday: Bool
        let items: [FundingActivityItem]
    }

    var days: [Day] {
        var result: [Day] = []
        for item in center.history {
            let title = FundingActivityDate.day(item.date)
            if let last = result.last, last.title == title {
                result[result.count - 1] = Day(title: title, isToday: last.isToday, items: last.items + [item])
            } else {
                result.append(Day(title: title, isToday: Calendar.current.isDateInToday(item.date), items: [item]))
            }
        }
        return result
    }

    var closingBalances: [Decimal] {
        guard let currentBalance else { return [] }

        let deltas = days.map { day in day.items.reduce(Decimal(0)) { $0 + $1.balanceChange(cash: cash) } }
        return FundingDayBalances.closing(current: currentBalance, dayChanges: deltas)
    }

    func dayMark(_ day: Day, balance: Decimal?) -> some View {
        GeometryReader { proxy in
            Color.clear.preference(
                key: PurseScroll.DaysKey.self,
                value: [
                    PurseScroll.DayMark(
                        title: day.title,
                        isToday: day.isToday,
                        balance: balance,
                        top: proxy.frame(in: .named(PurseScroll.space)).minY
                    )
                ]
            )
        }
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

/// The balance each history day closed on, newest day first: the balance now,
/// less what every later day changed.
enum FundingDayBalances {
    static func closing(current: Decimal, dayChanges: [Decimal]) -> [Decimal] {
        var balance = current
        return dayChanges.map { change in
            defer { balance -= change }
            return balance
        }
    }
}

extension FundingActivityItem {
    /// How much the session changed the balance once it ended.
    func balanceChange(cash: FundingCash) -> Decimal {
        guard let amount else { return 0 }

        switch status {
        case .toppedUp: return cash.decimal(amount)
        case .sent: return -cash.decimal(amount)
        case .inProgress,
             .refunded,
             .payoutFailed,
             .cancelled,
             .failed: return 0
        }
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

struct FundingActivityRow: View {
    let item: FundingActivityItem
    let cash: FundingCash
    var showsDivider = true
    /// How many sessions are in flight, drawn in the icon of the one on top.
    var stackCount: Int?

    static let height: CGFloat = 76
    private static let iconSize: CGFloat = 48

    var body: some View {
        let content = FundingActivityRowContent(item: item, cash: cash)
        HStack(spacing: DSSpacings.medium) {
            FundingActivityIcon(icon: content.icon, stackCount: stackCount)
                .frame(width: Self.iconSize, height: Self.iconSize)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: DSSpacings.smallIncreased) {
                    Text(content.title)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text(verbatim: content.amount)
                        .foregroundStyle(content.amountTone.color)
                        .lineLimit(1)
                }
                .typography(.bodyLargeEmphasized)
                .foregroundStyle(.fgPrimary)
                Text(content.subtitle)
                    .typography(.bodySmall)
                    .foregroundStyle(content.isSubtitleWarning ? Color.fgWarning : Color.fgSecondary)
                    .lineLimit(1)
            }
        }
        .frame(minHeight: Self.height)
        .overlay(alignment: .bottom) {
            if showsDivider {
                Divider()
                    .overlay(Color.strokePrimary)
                    .padding(.leading, Self.iconSize + DSSpacings.medium)
            }
        }
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
    let title: String
    let subtitle: String
    let isSubtitleWarning: Bool
    let amount: String
    let amountTone: Tone

    init(item: FundingActivityItem, cash: FundingCash, now: Date = Date(), calendar: Calendar = .current) {
        let isDelayed = item.isDelayed
        icon = Self.icon(item, isDelayed: isDelayed)
        title = Self.title(item)
        subtitle = Self.subtitle(item, cash: cash, now: now, calendar: calendar)
        isSubtitleWarning = isDelayed
        amount = Self.amount(item, cash: cash)
        amountTone = Self.tone(item)
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

    static func title(_ item: FundingActivityItem) -> String {
        switch item.status {
        case .inProgress:
            if item.direction == .in {
                String(localized: .Funding.activityToppingUp)
            } else {
                String(localized: .Funding.activitySending)
            }
        case .toppedUp: String(localized: .Funding.activityToppedUp)
        case .sent: String(localized: .Funding.activitySent)
        case .refunded: String(localized: .Funding.activityRefunded)
        case .payoutFailed: String(localized: .Funding.activityPayoutFailed)
        case .cancelled: String(localized: .Funding.activityCancelled)
        case .failed:
            if item.direction == .in {
                String(localized: .Funding.activityTopUpFailed)
            } else {
                String(localized: .Funding.activityWithdrawFailed)
            }
        }
    }

    static func subtitle(_ item: FundingActivityItem, cash: FundingCash, now: Date, calendar: Calendar) -> String {
        let moment = moment(item, now: now, calendar: calendar)
        switch item.status {
        case let .failed(failure):
            return failure.reasonText
        case let .inProgress(status):
            return inProgressText(status, cash: cash) ?? moment
        case .toppedUp,
             .sent,
             .refunded,
             .payoutFailed,
             .cancelled:
            return moment
        }
    }

    /// "{Rail} · {moment}", or the moment alone when the rail is unknown.
    static func moment(_ item: FundingActivityItem, now: Date, calendar: Calendar) -> String {
        let when = FundingActivityDate.moment(item.date, now: now, calendar: calendar)
        guard let rail = item.rail else { return when }
        return "\(rail.title) · \(when)"
    }

    static func inProgressText(_ status: FundingStatus, cash: FundingCash) -> String? {
        FundingCancelPresentation.line(for: status, compact: true)?.text
            ?? waitingText(status)
            ?? movingText(status, cash: cash)
    }

    /// The line for a session waiting on the user or the provider.
    static func waitingText(_ status: FundingStatus) -> String? {
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
    static func movingText(_ status: FundingStatus, cash: FundingCash) -> String? {
        switch status {
        case .convertingToCash: String(localized: .Funding.activityConverting(symbol: cash.symbol))
        case .transactionInitiated: String(localized: .Funding.activityInitiated)
        case .preparingPayout: String(localized: .Funding.activityPreparingPayout)
        case let .convertingTo(asset): String(localized: .Funding.activityConvertingOut(asset: asset))
        default: nil
        }
    }

    /// Signed while value moves or once it has, unsigned once nothing moved.
    static func amount(_ item: FundingActivityItem, cash: FundingCash) -> String {
        guard let units = item.amount else { return "" }

        let value = cash.decimal(units)
        switch item.status {
        case .refunded,
             .payoutFailed,
             .cancelled,
             .failed:
            return cash.label(value)
        case .inProgress,
             .toppedUp,
             .sent:
            return cash.signed(value, direction: item.direction)
        }
    }

    static func tone(_ item: FundingActivityItem) -> Tone {
        switch item.status {
        case .toppedUp: .success
        case .sent: .primary
        case .inProgress,
             .refunded,
             .payoutFailed,
             .cancelled,
             .failed: .secondary
        }
    }
}

/// A row's 48pt icon: a spinner while in flight, a card with an arrow once
/// value moved, a cross once it failed.
struct FundingActivityIcon: View {
    let icon: FundingActivityRowContent.Icon
    var stackCount: Int?

    var body: some View {
        ZStack {
            Circle().fill(background)
            glyph
        }
    }

    @ViewBuilder
    private var glyph: some View {
        switch icon {
        case let .pending(delayed):
            if let stackCount {
                FundingCountRing(count: stackCount)
            } else {
                ProgressView()
                    .tint(delayed ? Color.fgWarning : Color.fgPrimary)
            }
        case .received:
            FundingCardGlyph(direction: .in)
                .foregroundStyle(.fgPrimaryInverted)
        case .sent:
            FundingCardGlyph(direction: .out)
                .foregroundStyle(.fgPrimary)
        case .failed:
            Image(systemName: "xmark")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.fgError)
        }
    }

    private var background: Color {
        switch icon {
        case let .pending(delayed): delayed ? Color.bgStatusWarning.opacity(0.24) : Color.bgSurfaceNested
        case .received: .bgActionPrimary
        case .sent: .bgSurfaceNested
        case .failed: Color.bgStatusError.opacity(0.24)
        }
    }
}

/// A card outline with an arrow at its corner: into the card for value
/// coming in, out of it for value going out.
struct FundingCardGlyph: View {
    let direction: FundingDirection

    var body: some View {
        Canvas { context, size in
            let card = CGRect(x: 1, y: size.height * 0.3, width: size.width * 0.72, height: size.height * 0.52)
            context.stroke(
                Path(roundedRect: card, cornerRadius: 2.5),
                with: .foreground,
                style: StrokeStyle(lineWidth: 1.6)
            )
            let dot = CGRect(x: card.minX + 3, y: card.maxY - 6, width: 3, height: 3)
            context.fill(Path(ellipseIn: dot), with: .foreground)
            context.stroke(arrow(in: size), with: .foreground, style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
        }
        .frame(width: 22, height: 22)
        .accessibilityHidden(true)
    }

    private func arrow(in size: CGSize) -> Path {
        let outer = CGPoint(x: size.width - 1, y: 1)
        let inner = CGPoint(x: size.width * 0.62, y: size.height * 0.38)
        let (from, tip) = direction == .in ? (outer, inner) : (inner, outer)
        let head = size.width * 0.18
        let horizontal: CGFloat = tip.x > from.x ? -head : head
        let vertical: CGFloat = tip.y > from.y ? -head : head
        var path = Path()
        path.move(to: from)
        path.addLine(to: tip)
        path.move(to: CGPoint(x: tip.x + horizontal, y: tip.y))
        path.addLine(to: tip)
        path.addLine(to: CGPoint(x: tip.x, y: tip.y + vertical))
        return path
    }
}

/// How many sessions are in flight, inside a turning ring.
struct FundingCountRing: View {
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

/// The history's dates. A row's moment reads "Today at 08:25", "Yesterday at
/// 18:40", "4 Oct at 16:20", and "4 Oct 2025" for an earlier year; a day's
/// heading reads "Today", "Yesterday", "Sunday 4 October", and "Saturday 4
/// October 2025" for an earlier year.
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
            return String(localized: .Funding.dateDayAt(day: format(date, "d MMM", calendar: calendar), time: time))
        }
        return format(date, "d MMM yyyy", calendar: calendar)
    }

    /// The heading a day's rows sit under.
    static func day(_ date: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        if calendar.isDate(date, inSameDayAs: now) { return String(localized: .Funding.dateToday) }
        if isYesterday(date, now: now, calendar: calendar) { return String(localized: .Funding.dateYesterday) }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return format(date, "EEEE d MMMM", calendar: calendar)
        }
        return format(date, "EEEE d MMMM yyyy", calendar: calendar)
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
