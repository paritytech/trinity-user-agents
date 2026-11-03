import SwiftUI

/// Bar outline with the notch cut from its top edge at centre.
struct DSTabBarShape: Shape {
    func path(in rect: CGRect) -> Path {
        Path(roundedRect: rect, cornerRadius: DSTabBarStyle.cornerRadius, style: .continuous)
            .subtracting(.notchCut(in: rect))
    }
}

/// The brief's notch outline as offsets from the bar's centre line (x) and down from the bar row's top edge (y).
/// The bowl's radius and centre come from `DSTabBarStyle`, so the outline stays round when those change.
/// `blended(roundness:)` moves it toward the bowl circle as the shelf opens.
private enum NotchOutline {
    typealias Segment = (control1: CGPoint, control2: CGPoint, end: CGPoint)

    /// Where the cut meets the bar's top edge on the left. The right end is the last segment's `end`.
    static let start = CGPoint(x: -37, y: 0)

    /// Six cubic segments from the left top corner, round the bowl, to the right top corner.
    static var segments: [Segment] {
        let bowlRadius = DSTabBarStyle.notchRadius
        let bowlCentreY = DSTabBarStyle.notchCenterY
        let bowlBottomY = bowlCentreY + bowlRadius
        return [
            (CGPoint(x: -33, y: 0), CGPoint(x: -31, y: 5), CGPoint(x: -32, y: 9)),
            (CGPoint(x: -33, y: 13), CGPoint(x: -bowlRadius, y: 17), CGPoint(x: -bowlRadius, y: bowlCentreY)),
            (CGPoint(x: -bowlRadius, y: 41), CGPoint(x: -19, y: bowlBottomY), CGPoint(x: 0, y: bowlBottomY)),
            (CGPoint(x: 19, y: bowlBottomY), CGPoint(x: bowlRadius, y: 41), CGPoint(x: bowlRadius, y: bowlCentreY)),
            (CGPoint(x: bowlRadius, y: 17), CGPoint(x: 33, y: 13), CGPoint(x: 32, y: 9)),
            (CGPoint(x: 31, y: 5), CGPoint(x: 33, y: 0), CGPoint(x: 37, y: 0))
        ]
    }

    /// The outline at `roundness`: the brief's cut at 0, the bowl circle at 1, every point lerped between them.
    static func blended(roundness: CGFloat) -> (start: CGPoint, segments: [Segment]) {
        let bowl = bowlCircle
        let blendedSegments = zip(segments, bowl.segments).map { brief, circle in
            (
                control1: interpolate(brief.control1, circle.control1, roundness),
                control2: interpolate(brief.control2, circle.control2, roundness),
                end: interpolate(brief.end, circle.end, roundness)
            )
        }
        return (interpolate(start, bowl.start, roundness), blendedSegments)
    }

    /// The bowl circle from its left crossing of the row top, in the same six segments as `segments`. Its segment
    /// ends fall where the brief's do: the shoulders at y = 9 and the crossings of the row top.
    private static var bowlCircle: (start: CGPoint, segments: [Segment]) {
        let radius = DSTabBarStyle.notchRadius
        let centreY = DSTabBarStyle.notchCenterY
        let crossing = asin(-centreY / radius)
        let shoulder = asin((9 - centreY) / radius)
        let angles = [CGFloat.pi - crossing, .pi - shoulder, .pi, .pi / 2, 0, shoulder, crossing]
        return (bowlPoint(at: angles[0]), zip(angles, angles.dropFirst()).map { arcSegment(from: $0, to: $1) })
    }

    /// Point on the bowl circle at `angle` radians, measured with y pointing down.
    private static func bowlPoint(at angle: CGFloat) -> CGPoint {
        let radius = DSTabBarStyle.notchRadius
        let centreY = DSTabBarStyle.notchCenterY
        return CGPoint(x: radius * cos(angle), y: centreY + radius * sin(angle))
    }

    /// Cubic approximation of the bowl circle's arc between two angles. Each handle runs along the tangent for
    /// 4/3 · tan(sweep / 4) of the radius, which keeps the curve's midpoint on the circle.
    private static func arcSegment(from startAngle: CGFloat, to endAngle: CGFloat) -> Segment {
        let handle = 4.0 / 3.0 * tan((endAngle - startAngle) / 4) * DSTabBarStyle.notchRadius
        let start = bowlPoint(at: startAngle)
        let end = bowlPoint(at: endAngle)
        return (
            control1: CGPoint(x: start.x - handle * sin(startAngle), y: start.y + handle * cos(startAngle)),
            control2: CGPoint(x: end.x + handle * sin(endAngle), y: end.y - handle * cos(endAngle)),
            end: end
        )
    }

    /// Point `fraction` of the way from `from` to `to`.
    private static func interpolate(_ from: CGPoint, _ to: CGPoint, _ fraction: CGFloat) -> CGPoint {
        CGPoint(x: from.x + (to.x - from.x) * fraction, y: from.y + (to.y - from.y) * fraction)
    }
}

extension DSTabBarShape {
    /// The notch's edge from its left top corner round the bowl to its right top corner, left open.
    static func cutoutEdge(in rect: CGRect) -> Path {
        let rowTop = rect.maxY - DSTabBarStyle.height
        let roundness = min(max((rowTop - rect.minY) / DSTabBarStyle.notchOverhang, 0), 1)
        let outline = NotchOutline.blended(roundness: roundness)
        func point(_ offset: CGPoint) -> CGPoint {
            CGPoint(x: rect.midX + offset.x, y: rowTop + offset.y)
        }
        var edge = Path()
        edge.move(to: point(outline.start))
        for segment in outline.segments {
            edge.addCurve(to: point(segment.end), control1: point(segment.control1), control2: point(segment.control2))
        }
        return edge
    }
}

private extension Path {
    /// The notch the centre control sits in. The cut is positioned against the bar row at the bottom of the rect,
    /// not the top. The rect grows upward while the shelf is open, and the notch must stay with the row rather than
    /// float at the shelf's top edge. At rest the cut is the brief's outline. As the shelf opens, its lower edge blends
    /// toward the bowl circle, and once the shelf is `DSTabBarStyle.notchOverhang` tall the hole is a full round bowl.
    static func notchCut(in rect: CGRect) -> Path {
        let rowTop = rect.maxY - DSTabBarStyle.height
        var cut = DSTabBarShape.cutoutEdge(in: rect)
        cut.closeSubpath()
        guard rect.minY < rowTop else { return cut }
        return cut.union(bowlAboveRow(in: rect, rowTop: rowTop))
    }

    /// The part of the bowl above the bar row. While the shelf is open, the resting centre button's ring reaches into
    /// it, so the shelf must not paint over that ring.
    static func bowlAboveRow(in rect: CGRect, rowTop: CGFloat) -> Path {
        let radius = DSTabBarStyle.notchRadius
        let centre = CGPoint(x: rect.midX, y: rowTop + DSTabBarStyle.notchCenterY)
        let bowl = Path(ellipseIn: CGRect(
            x: centre.x - radius,
            y: centre.y - radius,
            width: radius * 2,
            height: radius * 2
        ))
        let aboveRow = Path(CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: rowTop - rect.minY))
        return bowl.intersection(aboveRow)
    }
}
