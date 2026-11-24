import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Masks travel in hundredths of a logical pixel; the widget draws in points.
extension Mask {
    var widgetMask: CustomMessageWidgetNode.Mask {
        CustomMessageWidgetNode.Mask(
            key: key,
            mode: mode == .show ? .show : .hide,
            shapes: shapes.map(\.widgetShape),
            // The iPhone stamped-brush layer stamps a followed brush; until then the strokes draw round.
            follow: follow.map { .init(pan: $0.pan, width: CGFloat($0.width) / 100) },
            fade: TimeInterval(fadeMs ?? 0) / 1_000,
            clearedShare: clearedShare.map { .init(atPermille: $0.atPermille, action: $0.action) }
        )
    }
}

extension MaskShape {
    var widgetShape: CustomMessageWidgetNode.Mask.Shape {
        switch self {
        case .all:
            .all
        case let .circle(center, radius):
            .circle(center: center.points, radius: CGFloat(radius) / 100)
        case let .stroke(width, points):
            .stroke(width: CGFloat(width) / 100, points: points.map(\.points))
        case let .stamped(width, _, points):
            // The iPhone stamped-brush layer stamps the tip; until then the line draws round at its width.
            .stroke(width: CGFloat(width) / 100, points: points.map(\.points))
        }
    }
}

private extension MaskPoint {
    var points: CGPoint { CGPoint(x: CGFloat(x) / 100, y: CGFloat(y) / 100) }
}
