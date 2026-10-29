import DesignSystem
import PolkadotUI
import SwiftUI

/// What a picked-out coin says about itself, beside it in the band: its value and whether it is
/// Ready or Clearing, how many owners it has had, and its quality, with a line of explanation
/// under the quality that the info mark shows and hides.
struct CoinageTraceView: View {
    let trace: CoinageCoinTrace
    /// Whether the quality is being explained. Held by the card so it resets with the pick.
    @Binding var isExplainingQuality: Bool

    private static let headline: CGFloat = 30
    private static let line: CGFloat = 24
    /// Four lines of the smaller type, which is what the explanation runs to beside the coin,
    /// and a little clear below so it does not sit on the band's edge.
    private static let explanation: CGFloat = 80
    private static let inset: CGFloat = CoinageArrangement.focusInset

    /// How tall the band has to be for the trace to fit beside the coin at its tallest: the two
    /// lines under the headline, a second line for the quality where a narrow screen wraps it,
    /// and the explanation's room. The band hangs from the foot of the screen, so it is placed
    /// for all of that from the start; the box inside it wraps its content and only reaches
    /// further down when the explanation opens.
    static func height(for _: CoinageCoinTrace) -> CGFloat {
        max(CoinageArrangement.focusBandMinimum, inset * 2 + headline + line * 3 + explanation)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: DSSpacings.extraSmall) {
                Text(trace.value)
                    .typography(.titleLarge)
                    .foregroundStyle(Color.fgPrimary)

                Text(String(localized: trace.partition == .ready ? .coinageSpendable : .coinageLoading))
                    .typography(.bodyMedium)
                    .foregroundStyle(Color.fgSecondary)
            }
            .frame(height: Self.headline)

            line(String(localized: .coinageTraceHops(trace.hops)))

            // The grade may not fit on one line beside the coin on a narrow screen, so this line
            // alone may take a second, and the mark runs on after the last word rather than
            // holding a place of its own that would push the grade down alone.
            Button {
                isExplainingQuality.toggle()
            } label: {
                (Text(String(localized: .coinageTraceQuality(Self.grade(trace.quality))))
                    + Text(verbatim: " ")
                    + Text(Image(systemName: isExplainingQuality ? "info.circle.fill" : "info.circle")))
                    .typography(.bodyMedium)
                    .foregroundStyle(Color.fgSecondary)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(minHeight: Self.line, alignment: .leading)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(.coinageTraceQualityInfo))

            if isExplainingQuality {
                explanation
                    .padding(.leading, DSSpacings.medium)
                    .padding(.top, DSSpacings.extraTiny)
                    .padding(.bottom, DSSpacings.extraSmall)
                    .transition(.opacity)
            }
        }
        .padding(.vertical, Self.inset)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var explanation: some View {
        (Text(.coinageTraceQualityInfo) + Text(verbatim: " ") + Text(.coinageTraceQualityInfoAside).font(Self.aside))
            .typography(.bodySmall)
            .foregroundStyle(Color.fgSecondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// The body face leaned over. The app ships no italic cut, so `.italic()` draws nothing;
    /// a slant on the regular face is what the aside gets instead.
    private static var aside: Font {
        let upright = UIFont.bodySmall
        let slant = CGAffineTransform(a: 1, b: 0, c: 0.2, d: 1, tx: 0, ty: 0)

        return Font(UIFont(descriptor: upright.fontDescriptor.withMatrix(slant), size: upright.pointSize))
    }

    private func line(_ text: String) -> some View {
        Text(text)
            .typography(.bodyMedium)
            .foregroundStyle(Color.fgSecondary)
            .lineLimit(1)
            .frame(height: Self.line)
    }

    private static func grade(_ quality: CoinageCoinTrace.Quality) -> String {
        switch quality {
        case .unknown: String(localized: .coinageTraceQualityUnknown)
        case .veryLow: String(localized: .coinageTraceQualityVeryLow)
        case .low: String(localized: .coinageTraceQualityLow)
        case .fair: String(localized: .coinageTraceQualityFair)
        case .good: String(localized: .coinageTraceQualityGood)
        case .high: String(localized: .coinageTraceQualityHigh)
        }
    }
}
