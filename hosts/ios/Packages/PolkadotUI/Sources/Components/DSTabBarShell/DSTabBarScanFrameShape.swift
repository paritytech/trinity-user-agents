import SwiftUI

/// The brief's four-bracket scan frame: an open bracket in each corner of a 24-unit grid, scaled uniformly to fit
/// the rect and centred in it.
struct DSTabBarScanFrameShape: Shape {
    /// Side of the square grid the brackets are drawn on.
    static let gridSide: CGFloat = 24
    /// Stroke width in grid units. It scales with the brackets, so the stroke keeps its proportion at any size.
    static let strokeWidth: CGFloat = 2.2

    /// Distance of each bracket's outer leg from the grid's edge.
    private static let edge: CGFloat = 1.3
    /// Radius of each bracket's corner arc.
    private static let arcRadius: CGFloat = 2.7
    /// Length of each bracket's straight leg, measured from its arc.
    private static let legLength: CGFloat = 3

    func path(in rect: CGRect) -> Path {
        let scale = min(rect.width, rect.height) / Self.gridSide
        let side = Self.gridSide * scale
        let transform = CGAffineTransform(translationX: rect.midX - side / 2, y: rect.midY - side / 2)
            .scaledBy(x: scale, y: scale)
        return Self.bracketsPath.applying(transform)
    }

    /// The four brackets in grid units. Each arc centre sits `arcRadius` in from its corner's two edges.
    private static var bracketsPath: Path {
        let near = edge + arcRadius
        let far = gridSide - edge - arcRadius
        let farEdge = gridSide - edge
        var path = Path()

        path.move(to: CGPoint(x: edge, y: near + legLength))
        path.addLine(to: CGPoint(x: edge, y: near))
        path.addArc(
            center: CGPoint(x: near, y: near),
            radius: arcRadius,
            startAngle: .degrees(180),
            endAngle: .degrees(270),
            clockwise: false
        )
        path.addLine(to: CGPoint(x: near + legLength, y: edge))

        path.move(to: CGPoint(x: far - legLength, y: edge))
        path.addLine(to: CGPoint(x: far, y: edge))
        path.addArc(
            center: CGPoint(x: far, y: near),
            radius: arcRadius,
            startAngle: .degrees(270),
            endAngle: .degrees(360),
            clockwise: false
        )
        path.addLine(to: CGPoint(x: farEdge, y: near + legLength))

        path.move(to: CGPoint(x: farEdge, y: far - legLength))
        path.addLine(to: CGPoint(x: farEdge, y: far))
        path.addArc(
            center: CGPoint(x: far, y: far),
            radius: arcRadius,
            startAngle: .degrees(0),
            endAngle: .degrees(90),
            clockwise: false
        )
        path.addLine(to: CGPoint(x: far - legLength, y: farEdge))

        path.move(to: CGPoint(x: near + legLength, y: farEdge))
        path.addLine(to: CGPoint(x: near, y: farEdge))
        path.addArc(
            center: CGPoint(x: near, y: far),
            radius: arcRadius,
            startAngle: .degrees(90),
            endAngle: .degrees(180),
            clockwise: false
        )
        path.addLine(to: CGPoint(x: edge, y: far - legLength))

        return path
    }
}
