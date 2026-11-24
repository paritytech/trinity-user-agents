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
            clearedShare: clearedShare.map(\.widgetShare),
            haptics: haptics.map {
                .init(everyPermille: $0.everyPermille, step: $0.step.widgetHaptic, reach: $0.reach?.widgetHaptic)
            }
        )
    }
}

extension ClearedShare {
    var widgetShare: CustomMessageWidgetNode.Mask.ClearedShare {
        CustomMessageWidgetNode.Mask.ClearedShare(
            atPermille: atPermille,
            action: action,
            whileDown: whileDown == true,
            rescue: rescue.map { .init(after: TimeInterval($0.afterMs) / 1_000, fromPermille: $0.fromPermille) }
        )
    }
}

extension Haptic {
    var widgetHaptic: CustomMessageWidgetNode.Haptic {
        switch self {
        case .tick: .tick
        case .light: .light
        case .medium: .medium
        case .heavy: .heavy
        case .success: .success
        }
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
