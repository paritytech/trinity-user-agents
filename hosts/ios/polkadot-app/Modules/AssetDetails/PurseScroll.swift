import DesignSystem
import PolkadotUI
import SwiftUI

/// Where the Purse's card sits while the wallet scrolls, so the sticky bar
/// knows when it has gone under it.
enum PurseScroll {
    /// The wallet's viewport, whose top is the bottom of the navigation bar.
    static let space = "purseScroll"
    static let barHeight: CGFloat = 49

    struct Card: Equatable {
        /// The card's top edge in ``PurseScroll/space``.
        var top: CGFloat = 0
        /// The card's bottom edge in ``PurseScroll/space``.
        let bottom: CGFloat
        let balance: Decimal?
        /// The card's frame in the window, where the shrinking card starts from.
        var frame: CGRect = .zero
    }

    struct Bar: Equatable {
        let balance: Decimal?
    }

    struct CardKey: PreferenceKey {
        static let defaultValue: Card? = nil

        static func reduce(value: inout Card?, nextValue: () -> Card?) {
            value = nextValue() ?? value
        }
    }

    /// The bar once the card has gone under it, with the current balance.
    /// Nil while the card is still in view.
    static func bar(card: Card?, threshold: CGFloat = barHeight) -> Bar? {
        guard let card, card.bottom < threshold else { return nil }
        return Bar(balance: card.balance)
    }
}

/// The bar that stands in for the card once it has scrolled away.
struct PurseStickyBar: View {
    let bar: PurseScroll.Bar
    let cash: FundingCash

    var body: some View {
        HStack {
            Text(.Funding.activityCurrentBalance)
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
}
