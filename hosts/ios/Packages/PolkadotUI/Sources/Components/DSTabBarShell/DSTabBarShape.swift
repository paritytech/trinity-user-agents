import SwiftUI

/// Bar outline with a circular hole at centre.
struct DSTabBarShape: Shape {
    func path(in rect: CGRect) -> Path {
        Path(roundedRect: rect, cornerRadius: DSTabBarStyle.cornerRadius, style: .continuous)
            .subtracting(.notchCut(in: rect))
    }
}

/// Everything outside the hole, extended past the bar by `DSTabBarStyle.shadowMaskSpread`.
/// Used as a mask: the bar's shadow is a blurred silhouette behind the bar, so without this
/// it would blur into the hole and show through it.
struct DSTabBarShadowMask: Shape {
    func path(in rect: CGRect) -> Path {
        let spread = DSTabBarStyle.shadowMaskSpread
        return Path(rect.insetBy(dx: -spread, dy: -spread)).subtracting(.notchCut(in: rect))
    }
}

private extension Path {
    /// The hole, plus the two corners where it meets the top edge of `rect`,
    /// rounded by `DSTabBarStyle.notchFilletRadius`.
    ///
    /// The hole is positioned against the bar row at the bottom of the rect, not the top. The rect
    /// grows upward while the shelf is open, and the hole must stay with the row rather than float
    /// at the shelf's top edge.
    static func notchCut(in rect: CGRect) -> Path {
        let radius = DSTabBarStyle.notchRadius
        let holeCentre = CGPoint(x: rect.midX, y: rect.maxY - DSTabBarStyle.height + DSTabBarStyle.notchCenterY)
        let hole = Path(ellipseIn: CGRect(
            x: holeCentre.x - radius,
            y: holeCentre.y - radius,
            width: radius * 2,
            height: radius * 2
        ))
        let overlap = rect.minY - (holeCentre.y - radius)
        let fillet = min(DSTabBarStyle.notchFilletRadius, overlap)
        guard fillet > 0 else { return hole }
        var cut = hole
        let sides: [CGFloat] = [-1, 1]
        for side in sides {
            cut = cut.union(filletCorner(side: side, holeCentre: holeCentre, edgeY: rect.minY, fillet: fillet))
        }
        return cut
    }

    /// One corner where the hole meets the top edge, as the region between the edge, the fillet arc and the hole.
    /// `side` is -1 for the left corner and 1 for the right.
    private static func filletCorner(side: CGFloat, holeCentre: CGPoint, edgeY: CGFloat, fillet: CGFloat) -> Path {
        let radius = DSTabBarStyle.notchRadius
        let filletCentreY = edgeY + fillet
        let holeOffsetY = holeCentre.y - filletCentreY
        let filletCentreDistance = radius + fillet
        let filletCentreX = holeCentre.x + side * sqrt(
            filletCentreDistance * filletCentreDistance - holeOffsetY * holeOffsetY
        )
        let filletCentre = CGPoint(x: filletCentreX, y: filletCentreY)
        let edgeTangent = CGPoint(x: filletCentreX, y: edgeY)
        let holeTangentAngle = atan2(holeCentre.y - filletCentreY, holeCentre.x - filletCentreX)

        var corner = Path()
        corner.move(to: edgeTangent)
        corner.addArc(
            center: filletCentre,
            radius: fillet,
            startAngle: .radians(-.pi / 2),
            endAngle: .radians(holeTangentAngle),
            clockwise: side > 0
        )
        corner.addLine(to: holeCentre)
        corner.addLine(to: CGPoint(x: holeCentre.x, y: edgeY))
        corner.closeSubpath()
        return corner
    }
}
