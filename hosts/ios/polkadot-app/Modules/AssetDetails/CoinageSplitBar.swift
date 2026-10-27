import DesignSystem
import SwiftUI

/// How much of the total is still clearing: a striped share at the start of a
/// light track, never thinner than a sliver while anything clears.
struct CoinageSplitBar: View {
    let clearing: Decimal
    let total: Decimal

    static let height: CGFloat = 6
    private static let minimumShare: CGFloat = 0.06

    /// How far through one stripe the scroll is at `date`, from 0 to 1.
    static func phase(at date: Date) -> CGFloat {
        let period = PurseMotion.Balance.stripePeriod
        return CGFloat(date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period) / period)
    }

    /// The clearing share of the bar's width, from 0 to 1.
    static func share(clearing: Decimal, total: Decimal) -> CGFloat {
        guard total > 0, clearing > 0 else { return 0 }

        let ratio = CGFloat(NSDecimalNumber(decimal: clearing / total).doubleValue)
        return min(max(ratio, minimumShare), 1)
    }

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Rectangle().fill(Color.fgPrimary)
                TimelineView(.animation(paused: reduceMotion)) { context in
                    ClearingStripes(phase: reduceMotion ? 0 : Self.phase(at: context.date))
                }
                .frame(width: proxy.size.width * Self.share(clearing: clearing, total: total))
                .clipped()
            }
        }
        .frame(height: Self.height)
        .clipShape(Capsule())
        .accessibilityHidden(true)
    }
}

/// Red diagonal stripes on the track's colour, moved along by `phase`.
private struct ClearingStripes: View {
    let phase: CGFloat

    private static let period: CGFloat = 8

    var body: some View {
        Canvas { context, size in
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.fgPrimary))
            var stripes = Path()
            var offset = -size.height - Self.period + phase * Self.period
            while offset < size.width + size.height {
                stripes.move(to: CGPoint(x: offset, y: size.height))
                stripes.addLine(to: CGPoint(x: offset + size.height, y: 0))
                offset += Self.period
            }
            context.stroke(stripes, with: .color(.fgError), lineWidth: Self.period / 2)
        }
    }
}
