import SwiftUI
import DesignSystem

/// The single place that says how an indication is drawn. A normal chain is monochrome — a filled
/// `.fgPrimary` disc with the icon knocked out — so anything muted on the strip reads as a chain
/// the app cannot reach.
enum ChainStatusRingStyle {
    static func isFilled(for indication: ChainStatusIndication) -> Bool {
        indication == .normal
    }

    /// How severe an outage arc looks. Split from the colour because a `Color` backed by a
    /// dynamic asset is not comparable — every access is a new instance — so the banding
    /// decision has to be its own value to be testable.
    enum ArcBand: Equatable {
        /// Red, under a quarter of the window's blocks.
        case critical
        /// Amber, under half.
        case degraded
        /// Green, under three quarters.
        case fair
        /// Monochrome. The ring pre-announces the healthy state before it fills at `5/6`.
        case nearNormal
    }

    static func arcBand(forLiveness liveness: Double) -> ArcBand {
        if liveness < 0.25 {
            .critical
        } else if liveness < 0.5 {
            .degraded
        } else if liveness < 0.75 {
            .fair
        } else {
            .nearNormal
        }
    }

    static func arcColor(for indication: ChainStatusIndication) -> Color {
        switch indication {
        case .normal:
            .fgPrimary
        case let .outage(liveness):
            color(for: arcBand(forLiveness: liveness))
        case .dead:
            .fgTertiary
        }
    }

    private static func color(for band: ArcBand) -> Color {
        switch band {
        case .critical:
            .bgStatusError
        case .degraded:
            .bgStatusWarning
        case .fair:
            .bgStatusSuccess
        case .nearNormal:
            .fgPrimary
        }
    }

    static func trackColor(for indication: ChainStatusIndication) -> Color {
        switch indication {
        case .normal:
            .fgPrimary.opacity(0.2)
        case .outage:
            .fgPrimary.opacity(0.2)
        case .dead:
            .fgTertiary
        }
    }

    static func iconColor(for indication: ChainStatusIndication) -> Color {
        switch indication {
        case .normal:
            .bgSurfaceMain
        case .outage:
            .fgTertiary
        case .dead:
            .fgTertiary
        }
    }
}
