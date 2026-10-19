import DesignSystem
import PolkadotUI
import SwiftUI

/// Where the Purse's card and history days sit while the wallet scrolls, so the
/// sticky bar knows what has gone under it.
enum PurseScroll {
    /// The wallet's viewport, whose top is the bottom of the navigation bar.
    static let space = "purseScroll"
    static let barHeight: CGFloat = 49

    struct Card: Equatable {
        /// The card's top edge in ``PurseScroll/space``, as laid out before
        /// it shrinks.
        var top: CGFloat = 0
        /// The card's bottom edge in ``PurseScroll/space``.
        let bottom: CGFloat
        let balance: Decimal?
    }

    struct DayMark: Equatable {
        let title: String
        let isToday: Bool
        /// The balance once the day was over.
        let balance: Decimal?
        /// The heading's top edge in ``PurseScroll/space``.
        let top: CGFloat
    }

    struct Bar: Equatable {
        enum Label: Equatable {
            case currentBalance
            case day(String)
        }

        let label: Label
        let balance: Decimal?
    }

    struct CardKey: PreferenceKey {
        static let defaultValue: Card? = nil

        static func reduce(value: inout Card?, nextValue: () -> Card?) {
            value = nextValue() ?? value
        }
    }

    struct DaysKey: PreferenceKey {
        static let defaultValue: [DayMark] = []

        static func reduce(value: inout [DayMark], nextValue: () -> [DayMark]) {
            value.append(contentsOf: nextValue())
        }
    }

    /// The bar once the card has gone under it: the current balance, or the
    /// latest day whose heading has scrolled up to it with that day's closing
    /// balance. Nil while the card is still in view.
    static func bar(card: Card?, days: [DayMark], threshold: CGFloat = barHeight) -> Bar? {
        guard let card, card.bottom < threshold else { return nil }

        let passed = days.filter { $0.top <= threshold }.max { $0.top < $1.top }
        guard let passed, !passed.isToday else {
            return Bar(label: .currentBalance, balance: card.balance)
        }
        return Bar(label: .day(passed.title), balance: passed.balance)
    }
}

/// The bar that stands in for the card once it has scrolled away.
struct PurseStickyBar: View {
    let bar: PurseScroll.Bar
    let cash: FundingCash

    var body: some View {
        HStack {
            Text(label)
            Spacer(minLength: DSSpacings.small)
            if let balance = bar.balance {
                Text(verbatim: cash.label(balance))
                    .monospacedDigit()
            }
        }
        .typography(.bodyLargeEmphasized)
        .foregroundStyle(.fgPrimary)
        .padding(.horizontal, DSSpacings.large)
        .frame(height: PurseScroll.barHeight)
        .background(.bgSurfaceMain)
    }

    private var label: String {
        switch bar.label {
        case .currentBalance: String(localized: .Funding.activityCurrentBalance)
        case let .day(title): title
        }
    }
}
