import SwiftUI

public extension CustomMessageWidgetNode.Mask {
    /// A shaped tip for a mask stroke (`MaskBrush`), such as a slanted nib: copies of the tip stamped along the line
    /// instead of a round pen. The tip is the stroke's width long along its own axis and `aspect` of that across,
    /// turned `angle` from the horizontal, and keeps that turn whichever way the line goes.
    struct Brush: Equatable {
        /// The most stamps one line has (`MASK_STAMPS_MAX`).
        public static let mostStamps = 4_096
        /// The least distance between two stamps, in points (`MASK_STAMP_SPACING_MIN`, 50 hundredths).
        public static let leastSpacing: CGFloat = 0.5

        public enum Tip: Equatable {
            case ellipse
            /// A rectangle with fully round ends, as long and as thick as the ellipse.
            case capsule
        }

        public let tip: Tip
        /// The thickness across the tip as a share of its length, 0.001 through 1.
        public let aspect: CGFloat
        /// Radians, clockwise from the horizontal, as the y axis points down.
        public let angle: CGFloat
        /// The distance between stamp centres as a share of the length, 0.05 through 4.
        public let spacing: CGFloat

        public init(tip: Tip, aspect: CGFloat, angle: CGFloat, spacing: CGFloat) {
            self.tip = tip
            self.aspect = min(max(aspect, 0.001), 1)
            self.angle = angle
            self.spacing = min(max(spacing, 0.05), 4)
        }

        /// The stamps' centres along `points` for a tip `width` long, as the core places them (`MaskBrush::stamps`):
        /// the first on the first point, each next one a spacing further along the line, the distance carried across
        /// its corners, and one more on a last point the spacing leaves bare. Stamps are never closer than
        /// `leastSpacing`, and a line has at most `mostStamps`.
        public func stamps(width: CGFloat, along points: [CGPoint]) -> [CGPoint] {
            guard let first = points.first, let last = points.last else { return [] }
            let segments = zip(points, points.dropFirst())
            let total = segments.reduce(0) { $0 + hypot($1.1.x - $1.0.x, $1.1.y - $1.0.y) }
            // A line longer than the most stamps allow has its spacing stretched to fit.
            let step = max(width * spacing, Self.leastSpacing, total / CGFloat(Self.mostStamps - 1))
            var centres = [first]
            var sinceLast: CGFloat = 0
            for (start, end) in segments {
                let length = hypot(end.x - start.x, end.y - start.y)
                var along = step - sinceLast
                while along <= length, centres.count < Self.mostStamps {
                    let share = along / length
                    centres.append(CGPoint(
                        x: start.x + share * (end.x - start.x),
                        y: start.y + share * (end.y - start.y)
                    ))
                    along += step
                }
                sinceLast = length - (along - step)
            }
            if sinceLast > 0, centres.count < Self.mostStamps { centres.append(last) }
            return centres
        }

        /// Whether `point` lies in the stamp centred at `centre` for a tip `width` long, edge included.
        public func covers(width: CGFloat, centre: CGPoint, point: CGPoint) -> Bool {
            let halfLength = width / 2
            let halfThickness = halfLength * aspect
            let dx = point.x - centre.x, dy = point.y - centre.y
            let along = dx * cos(angle) + dy * sin(angle)
            let across = -dx * sin(angle) + dy * cos(angle)
            switch tip {
            case .ellipse:
                return square(along / halfLength) + square(across / halfThickness) <= 1
            case .capsule:
                let beyond = max(abs(along) - max(halfLength - halfThickness, 0), 0)
                return square(beyond) + square(across) <= square(halfThickness)
            }
        }

        /// One stamp's outline at `centre`; a right-to-left node draws it turned the other way, as its places are
        /// counted from its right edge.
        func outline(width: CGFloat, at centre: CGPoint, mirrored: Bool) -> Path {
            let thickness = width * aspect
            let rect = CGRect(x: -width / 2, y: -thickness / 2, width: width, height: thickness)
            let shape = tip == .ellipse
                ? Path(ellipseIn: rect)
                : Path(roundedRect: rect, cornerRadius: thickness / 2, style: .circular)
            let turn = CGAffineTransform(rotationAngle: mirrored ? -angle : angle)
            return shape.applying(turn.concatenating(CGAffineTransform(translationX: centre.x, y: centre.y)))
        }

        private func square(_ value: CGFloat) -> CGFloat {
            value * value
        }
    }
}
