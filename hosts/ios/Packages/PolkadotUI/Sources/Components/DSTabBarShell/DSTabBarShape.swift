import SwiftUI

/// Bar outline with a circular hole at centre.
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
    /// Adds the hole as a second subpath, so callers must fill or clip with `FillStyle(eoFill: true)`.
    /// Under the default winding fill the hole reads as a filled disc instead.
    ///
    /// The hole is positioned against the bar row at the bottom of the rect, not the top. The rect
    /// grows upward while the shelf is open, and the hole must stay with the row rather than float
    /// at the shelf's top edge.
    mutating func addNotch(in rect: CGRect) {
        let radius = DSTabBarStyle.notchRadius
        let rowTop = rect.maxY - DSTabBarStyle.height
        let circleTop = rowTop + DSTabBarStyle.notchCenterY - radius
        addEllipse(in: CGRect(x: rect.midX - radius, y: circleTop, width: radius * 2, height: radius * 2))
    }
}
