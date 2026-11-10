import SwiftUI

/// Rounded rectangular body with a lobe hanging below the middle of its bottom edge. The lobe holds the centre glyph.
///
/// Drawn as a single outline rather than a rect unioned with a shape, so it fills and clips as one subpath. The
/// outline comes from the rect alone, so the resting button and the open panel are this shape at different sizes.
/// Below the brief's 88 pt card width the outline eases toward a circle, so the 64 pt resting button is an exact
/// circle and the centre control's frame animation carries the outline through the blend without jumps.
struct DSTabBarMorphShape: InsettableShape {
    /// Width of the brief's card outline. From here up the outline is the brief's card.
    private static let cardWidth = DSTabBarStyle.centerLobeHalfWidth * 2
    /// Width of the resting centre button. At or below it the outline is a circle.
    private static let restingWidth = DSTabBarStyle.centerButtonSize

    /// How far `inset(by:)` has pulled the outline in, so a border stroke stays inside the shape.
    var insetAmount: CGFloat = 0

    func inset(by amount: CGFloat) -> DSTabBarMorphShape {
        var shape = self
        shape.insetAmount += amount
        return shape
    }

    func path(in rect: CGRect) -> Path {
        let rect = rect.insetBy(dx: insetAmount, dy: insetAmount)
        let roundness = min(1, max(0, (Self.cardWidth - rect.width) / (Self.cardWidth - Self.restingWidth)))
        let bodyHeight = rect.height - lerp(DSTabBarStyle.centerLobeDepth, rect.width / 2, roundness)
        guard rect.width > 0, bodyHeight > 0 else { return Path() }

        let cornerRadius = min(
            DSTabBarStyle.centerPanelCornerRadius,
            rect.width / 2,
            lerp(bodyHeight / 2, bodyHeight, roundness)
        )
        let lobeScale = min(1, rect.width / Self.cardWidth)
        let lobeHalfWidth = lerp(DSTabBarStyle.centerLobeHalfWidth * lobeScale, rect.width / 2, roundness)
        // The lobe takes the middle of the bottom edge, so the bottom corners only have the width either side of it.
        let bottomCornerRadius = max(0, min(cornerRadius, rect.width / 2 - lobeHalfWidth))

        let bottom = rect.minY + bodyHeight
        let left = rect.minX
        let right = rect.maxX
        let centreX = rect.midX

        var path = Path()
        path.addTopCorners(of: rect, cornerRadius: cornerRadius)
        path.addLine(to: CGPoint(x: right, y: bottom - bottomCornerRadius))
        path.addArc(
            center: CGPoint(x: right - bottomCornerRadius, y: bottom - bottomCornerRadius),
            radius: bottomCornerRadius,
            startAngle: .degrees(0),
            endAngle: .degrees(90),
            clockwise: false
        )
        path.addLine(to: CGPoint(x: centreX + lobeHalfWidth, y: bottom))
        path.addLobe(centre: CGPoint(x: centreX, y: bottom), width: rect.width, roundness: roundness)
        path.addLine(to: CGPoint(x: left + bottomCornerRadius, y: bottom))
        path.addArc(
            center: CGPoint(x: left + bottomCornerRadius, y: bottom - bottomCornerRadius),
            radius: bottomCornerRadius,
            startAngle: .degrees(90),
            endAngle: .degrees(180),
            clockwise: false
        )
        path.closeSubpath()
        return path
    }
}

/// One cubic segment of the lobe. Its start is the previous segment's end.
private struct LobeCurve {
    let control1: CGPoint
    let control2: CGPoint
    let end: CGPoint
}

private func lerp(_ from: CGFloat, _ to: CGFloat, _ amount: CGFloat) -> CGFloat {
    from + (to - from) * amount
}

private func blend(_ from: CGPoint, _ to: CGPoint, _ amount: CGFloat) -> CGPoint {
    CGPoint(x: lerp(from.x, to.x, amount), y: lerp(from.y, to.y, amount))
}

/// The brief's four lobe cubics from the body's right end to its left end. Its x offsets scale with the panel width.
private func briefLobeCurves(centre: CGPoint, scale: CGFloat) -> [LobeCurve] {
    func point(_ offsetX: CGFloat, _ offsetY: CGFloat) -> CGPoint {
        CGPoint(x: centre.x + offsetX * scale, y: centre.y + offsetY)
    }
    let depth = DSTabBarStyle.centerLobeDepth
    return [
        LobeCurve(control1: point(37, 0), control2: point(32, 5), end: point(30, 11)),
        LobeCurve(control1: point(26, 23), control2: point(13, depth), end: point(0, depth)),
        LobeCurve(control1: point(-13, depth), control2: point(-26, 23), end: point(-30, 11)),
        LobeCurve(
            control1: point(-32, 5),
            control2: point(-37, 0),
            end: point(-DSTabBarStyle.centerLobeHalfWidth, 0)
        ),
    ]
}

/// The lower half of a circle as four arcs split where the brief's shoulders sit, so each brief cubic pairs with
/// the arc covering the same stretch of the outline.
private func circleLobeCurves(centre: CGPoint, radius: CGFloat) -> [LobeCurve] {
    let shoulder: CGFloat = atan2(11, 30)
    return [
        arcCurve(centre: centre, radius: radius, from: 0, to: shoulder),
        arcCurve(centre: centre, radius: radius, from: shoulder, to: .pi / 2),
        arcCurve(centre: centre, radius: radius, from: .pi / 2, to: .pi - shoulder),
        arcCurve(centre: centre, radius: radius, from: .pi - shoulder, to: .pi),
    ]
}

/// Cubic approximation of an arc from `start` to `end` radians, increasing clockwise on screen.
private func arcCurve(centre: CGPoint, radius: CGFloat, from start: CGFloat, to end: CGFloat) -> LobeCurve {
    func point(_ angle: CGFloat) -> CGPoint {
        CGPoint(x: centre.x + radius * cos(angle), y: centre.y + radius * sin(angle))
    }
    let handle = 4 / 3 * tan((end - start) / 4) * radius
    return LobeCurve(
        control1: CGPoint(x: point(start).x - handle * sin(start), y: point(start).y + handle * cos(start)),
        control2: CGPoint(x: point(end).x + handle * sin(end), y: point(end).y - handle * cos(end)),
        end: point(end)
    )
}

private extension Path {
    /// Draws the top edge from the body's left side over both top corners to its right side. The corners are
    /// circular arcs of `cornerRadius`.
    mutating func addTopCorners(of rect: CGRect, cornerRadius: CGFloat) {
        let top = rect.minY
        move(to: CGPoint(x: rect.minX, y: top + cornerRadius))
        addArc(
            center: CGPoint(x: rect.minX + cornerRadius, y: top + cornerRadius),
            radius: cornerRadius,
            startAngle: .degrees(180),
            endAngle: .degrees(270),
            clockwise: false
        )
        addLine(to: CGPoint(x: rect.maxX - cornerRadius, y: top))
        addArc(
            center: CGPoint(x: rect.maxX - cornerRadius, y: top + cornerRadius),
            radius: cornerRadius,
            startAngle: .degrees(270),
            endAngle: .degrees(0),
            clockwise: false
        )
    }

    /// Draws the lobe from the body's right end, round its bottom, to the body's left end. Each brief cubic is
    /// blended toward its circle arc by `roundness`, so a 64 pt button is a circle and a 88 pt panel is the brief.
    mutating func addLobe(centre: CGPoint, width: CGFloat, roundness: CGFloat) {
        let scale = min(1, width / (DSTabBarStyle.centerLobeHalfWidth * 2))
        let briefCurves = briefLobeCurves(centre: centre, scale: scale)
        let circleCurves = circleLobeCurves(centre: centre, radius: width / 2)
        for (brief, circle) in zip(briefCurves, circleCurves) {
            addCurve(
                to: blend(brief.end, circle.end, roundness),
                control1: blend(brief.control1, circle.control1, roundness),
                control2: blend(brief.control2, circle.control2, roundness)
            )
        }
    }
}
