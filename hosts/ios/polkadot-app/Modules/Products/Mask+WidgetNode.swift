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
            follow: follow.map { .init(pan: $0.pan, width: CGFloat($0.width) / 100, brush: $0.brush?.widgetBrush) },
            fade: TimeInterval(fadeMs ?? 0) / 1_000,
            // The iPhone mask-progress layer checks mid-stroke, rescues a stalled share and plays `haptics`; until
            // then `whileDown`, `rescue` and `haptics` are ignored.
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
        case let .stamped(width, brush, points):
            .stamped(width: CGFloat(width) / 100, brush: brush.widgetBrush, points: points.map(\.points))
        }
    }
}

extension MaskBrush {
    var widgetBrush: CustomMessageWidgetNode.Mask.Brush {
        CustomMessageWidgetNode.Mask.Brush(
            tip: tip == .capsule ? .capsule : .ellipse,
            aspect: CGFloat(aspectPermille) / 1_000,
            angle: CGFloat(angleCentideg) / 100 * .pi / 180,
            spacing: CGFloat(spacingPermille) / 1_000
        )
    }
}

private extension MaskPoint {
    var points: CGPoint { CGPoint(x: CGFloat(x) / 100, y: CGFloat(y) / 100) }
}
