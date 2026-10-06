import DesignSystem
import PolkadotUI
import SwiftUI

/// Rules under the summary strip saying which coins are Clearing and which are Ready.
///
/// Replaces the composition bar. The coins are already grouped into the two partitions, so a bar
/// above them said the same thing twice; a rule under each run says it where the run is. The
/// textures are the bar's own — solid for Ready, barber pole for Clearing — so what the bar taught
/// still reads, in the place it now applies to.
///
/// The one thing the bar did that this cannot is weight by value: it is one coin per coin, so a
/// single large coin clearing next to many small ready ones reads as a short run. The amount beside
/// each label is there for that, and says it exactly rather than proportionally.
struct CoinageRunMarkers: View {
    struct Run: Equatable, Identifiable {
        let partition: CoinageStripLayout.Partition
        /// Where the run reaches, in the strip's own coordinates.
        let start: CGFloat
        let end: CGFloat
        let title: String
        let amount: String

        var id: String { partition.rawValue }
        var centre: CGFloat { (start + end) / 2 }
    }

    let runs: [Run]
    let width: CGFloat

    static let height: CGFloat = 30
    private static let rule: CGFloat = 4
    /// Both textures are mostly white, and the card behind them is not always dark. Half a point,
    /// inset, so the outline reads without eating the pattern it is there to make visible.
    private static let outline: CGFloat = 0.5
    private static let labelTop: CGFloat = 7
    /// Between two labels that have both been pushed toward the middle.
    private static let labelGap: CGFloat = 12

    @State private var labelWidths: [String: CGFloat] = [:]

    var body: some View {
        let placed = Self.centres(for: runs, widths: runs.map { labelWidths[$0.id] ?? 0 }, in: width)

        ZStack(alignment: .topLeading) {
            ForEach(Array(zip(runs, placed)), id: \.0.id) { run, centre in
                texture(for: run.partition)
                    .frame(width: max(run.end - run.start, Self.rule), height: Self.rule)
                    .clipShape(Capsule())
                    // A capsule rather than a slight radius: at four points tall anything less is
                    // indistinguishable, and fully rounded ends read as a deliberate stop.
                    .overlay(Capsule().strokeBorder(Color.black, lineWidth: Self.outline))
                    .offset(x: run.start)

                label(for: run)
                    .fixedSize()
                    .background(
                        GeometryReader { proxy in
                            Color.clear.preference(key: LabelWidths.self, value: [run.id: proxy.size.width])
                        }
                    )
                    .offset(x: centre - (labelWidths[run.id] ?? 0) / 2, y: Self.labelTop)
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .frame(height: Self.height, alignment: .top)
        .onPreferenceChange(LabelWidths.self) { labelWidths = $0 }
    }

    private func label(for run: Run) -> some View {
        HStack(spacing: DSSpacings.extraTiny) {
            Text(run.title)
                .foregroundStyle(Color.fgSecondary)

            Text(run.amount)
                .foregroundStyle(Color.fgPrimary)
        }
        .typography(.bodySmall)
        .lineLimit(1)
    }

    @ViewBuilder
    private func texture(for partition: CoinageStripLayout.Partition) -> some View {
        switch partition {
        case .ready: Color.fgStaticWhite
        case .clearing: DSBarberPole()
        }
    }
}

// MARK: - Placing the labels

extension CoinageRunMarkers {
    /// Where each label sits: under the middle of its run, but never past the margins and never on
    /// top of its neighbour.
    ///
    /// A run of one coin at the very edge would otherwise hang its label over the side, and two
    /// short runs at opposite ends both pull toward the middle. Forwards to separate them, then
    /// backwards from the right edge, which is the usual way to settle a row of labels that each
    /// want a place and together may not fit.
    static func centres(for runs: [Run], widths: [CGFloat], in width: CGFloat) -> [CGFloat] {
        guard !runs.isEmpty, width > 0 else { return runs.map(\.centre) }

        var centres = zip(runs, widths).map { run, label in
            min(max(run.centre, label / 2), max(width - label / 2, label / 2))
        }

        for index in 1 ..< max(centres.count, 1) {
            let room = (widths[index - 1] + widths[index]) / 2 + labelGap
            centres[index] = max(centres[index], centres[index - 1] + room)
        }

        for index in stride(from: centres.count - 1, through: 0, by: -1) {
            centres[index] = min(centres[index], width - widths[index] / 2)

            if index > 0 {
                let room = (widths[index - 1] + widths[index]) / 2 + labelGap
                centres[index - 1] = min(centres[index - 1], centres[index] - room)
            }
        }

        return zip(centres, widths).map { centre, label in max(centre, label / 2) }
    }
}

private struct LabelWidths: PreferenceKey {
    static let defaultValue: [String: CGFloat] = [:]

    static func reduce(value: inout [String: CGFloat], nextValue: () -> [String: CGFloat]) {
        value.merge(nextValue()) { _, new in new }
    }
}
