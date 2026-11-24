import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Parallax travels in thousandths, hundredths of a logical pixel and of a degree, and milliseconds; the widget moves
/// in points, degrees and seconds.
extension Parallax {
    var widgetParallax: CustomMessageWidgetNode.Parallax {
        CustomMessageWidgetNode.Parallax(
            depth: Double(depthPermille) / 1_000,
            tilt: tilt.map {
                .init(
                    travel: CGFloat($0.travel) / 100,
                    maxTilt: Double($0.maxTiltCentideg) / 100,
                    settle: TimeInterval($0.settleMs) / 1_000
                )
            },
            pan: pan
        )
    }
}
