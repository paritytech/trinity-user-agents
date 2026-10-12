import SwiftUI
import DesignSystem

/// Per-chain readout of every monitored connection. Displays the ring at a larger size plus
/// a metrics line of connection state, liveness and average block interval.
public struct ConnectionStatusPanelView: View, Hashable {
    public let rows: [ChainConnectionStatusViewModel]

    public init(rows: [ChainConnectionStatusViewModel]) {
        self.rows = rows
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.large) {
            Text(.Common.connectionStatusPanelTitle)
                .typography(.titleLarge)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(1)

            ForEach(rows) { row in
                rowView(row)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(DSSpacings.mediumIncreased)
    }
}

private extension ConnectionStatusPanelView {
    static let placeholder = "—"

    static let separator = " · "

    /// Matches the ring's arc animation so a liveness change reads as one movement across the row.
    static let metricsAnimation: Animation = .easeOut(duration: 0.3)

    static let intervalFormatter = DateComponentsFormatter.secondsMinutesAbbreviated

    func rowView(_ row: ChainConnectionStatusViewModel) -> some View {
        HStack(spacing: DSSpacings.small) {
            ChainStatusRingView(viewModel: row, diameter: 35)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: DSSpacings.zero) {
                Text(verbatim: row.title)
                    .typography(.titleMedium)
                    .foregroundStyle(Color.fgPrimary)
                    .lineLimit(1)

                HStack(spacing: 0) {
                    Text(verbatim: row.stateTitle)

                    if row.showsChainMetrics, row.state != .offline {
                        Text(verbatim: "\(Self.separator)\(livenessText(row))")
                            .contentTransition(.numericText())

                        Text(verbatim: "\(Self.separator)\(blockIntervalText(row))")
                            .contentTransition(.numericText())
                    }
                }
                .typography(.bodySmall)
                .monospacedDigit()
                .foregroundStyle(Color.fgSecondary)
                .lineLimit(1)
                .animation(Self.metricsAnimation, value: row.liveness)
            }

            Spacer(minLength: 0)
        }
    }

    func livenessText(_ row: ChainConnectionStatusViewModel) -> String {
        let percent = row.liveness.map { $0.formatted(.percent.precision(.fractionLength(0))) }
        return String(localized: .Common.connectionStatusLivenessValue(percent ?? Self.placeholder))
    }

    func blockIntervalText(_ row: ChainConnectionStatusViewModel) -> String {
        let label = String(localized: .Common.connectionStatusBlockLabel)

        guard
            let liveness = row.liveness,
            liveness > 0,
            let interval = Self.intervalFormatter.string(from: row.expectedBlockSeconds / liveness)
        else {
            return "\(label) \(Self.placeholder)"
        }

        return "\(label) ~\(interval)"
    }
}
