import SwiftUI

/// Bar outline with a circular hole at centre. Filled with `FillStyle(eoFill: true)` so the
/// hole reads as empty rather than as a second filled disc.
struct DSTabBarShape: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path(roundedRect: rect, cornerRadius: DSTabBarStyle.cornerRadius, style: .continuous)
        path.addNotch(in: rect)
        return path
    }
}

/// Everything outside the hole, extended past the bar by `DSTabBarStyle.shadowMaskSpread`.
/// Used as a mask: the bar's shadow is a blurred silhouette behind the bar, so without this
/// it would blur into the hole and show through it.
struct DSTabBarShadowMask: Shape {
    func path(in rect: CGRect) -> Path {
        let spread = DSTabBarStyle.shadowMaskSpread
        var path = Path(rect.insetBy(dx: -spread, dy: -spread))
        path.addNotch(in: rect)
        return path
    }
}

private extension Path {
    /// Adds the hole as its own subpath. It only reads as a hole under an even-odd fill.
    mutating func addNotch(in rect: CGRect) {
        let radius = DSTabBarStyle.notchRadius
        let circleTop = rect.minY + DSTabBarStyle.notchCenterY - radius
        addEllipse(in: CGRect(x: rect.midX - radius, y: circleTop, width: radius * 2, height: radius * 2))
    }
}
