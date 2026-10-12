import SwiftUI

/// Rectangular body with a semicircular lobe hanging below its bottom edge.
///
/// Drawn as a single outline rather than a rect unioned with a circle, so it fills and clips as
/// one subpath. At `width == height`, `topRadius == lobeRadius == width / 2` and a zero bottom
/// radius, the outline is a circle, so the resting button and the open panel are the same shape
/// at different sizes.
struct DSTabBarMorphShape: InsettableShape {
    /// Corners of the body's top edge.
    var topRadius: CGFloat
    /// Corners where the body's bottom edge meets its sides.
    var bottomRadius: CGFloat
    /// The lobe below the body. Keeps the button's size through the morph.
    var lobeRadius: CGFloat
    /// Concave blend where the lobe meets the body's bottom edge.
    var filletRadius: CGFloat
    /// How far `inset(by:)` has pulled the outline in, so a border stroke stays inside the shape.
    var insetAmount: CGFloat = 0

    var animatableData: AnimatablePair<AnimatablePair<CGFloat, CGFloat>, AnimatablePair<CGFloat, CGFloat>> {
        get {
            AnimatablePair(
                AnimatablePair(topRadius, bottomRadius),
                AnimatablePair(lobeRadius, filletRadius)
            )
        }
        set {
            topRadius = newValue.first.first
            bottomRadius = newValue.first.second
            lobeRadius = newValue.second.first
            filletRadius = newValue.second.second
        }
    }

    func inset(by amount: CGFloat) -> DSTabBarMorphShape {
        var shape = self
        shape.insetAmount += amount
        return shape
    }

    func path(in rect: CGRect) -> Path {
        let rect = rect.insetBy(dx: insetAmount, dy: insetAmount)
        let topRadius = max(0, topRadius - insetAmount)
        let bottomRadius = max(0, bottomRadius - insetAmount)
        let lobeRadius = max(0, lobeRadius - insetAmount)
        let filletRadius = filletRadius + insetAmount

        var path = Path()

        guard rect.width > 0, rect.height > 0 else { return path }

        let lobe = max(0, min(lobeRadius, rect.width / 2, rect.height / 2))
        let bodyBottom = rect.maxY - lobe
        let bodyHeight = bodyBottom - rect.minY
        // The lobe eats the middle of the bottom edge, so the bottom corners
        // only have the width left either side of it.
        let bottom = max(0, min(bottomRadius, rect.width / 2 - lobe, bodyHeight))
        let top = max(0, min(topRadius, rect.width / 2, bodyHeight - bottom))

        path.addTopAndRightSide(in: rect, top: top, bottom: bottom, bodyBottom: bodyBottom)

        // The blend is a skirt: the bottom edge leaves the straight run curving
        // downward and meets the lobe tangentially. Its centre sits f below the
        // edge and lobe + f from the lobe's centre, so the join is wider than
        // the lobe and must still clear the bottom corners.
        let span = rect.width / 2 - bottom
        let widthLimit = lobe > 0 ? (span * span - lobe * lobe) / (2 * lobe) : 0
        let fillet = max(0, min(filletRadius, widthLimit, lobe))
        path.addLobe(in: rect, lobe: lobe, fillet: fillet, bodyBottom: bodyBottom)

        path.addLine(to: CGPoint(x: rect.minX + bottom, y: bodyBottom))
        path.addArc(
            center: CGPoint(x: rect.minX + bottom, y: bodyBottom - bottom),
            radius: bottom,
            startAngle: .degrees(90),
            endAngle: .degrees(180),
            clockwise: false
        )
        path.closeSubpath()
        return path
    }
}

private extension Path {
    /// Runs clockwise from the left side's top corner, down the right side, and ends where the
    /// bottom-right corner meets the bottom edge.
    mutating func addTopAndRightSide(in rect: CGRect, top: CGFloat, bottom: CGFloat, bodyBottom: CGFloat) {
        move(to: CGPoint(x: rect.minX, y: rect.minY + top))
        addArc(
            center: CGPoint(x: rect.minX + top, y: rect.minY + top),
            radius: top,
            startAngle: .degrees(180),
            endAngle: .degrees(270),
            clockwise: false
        )
        addLine(to: CGPoint(x: rect.maxX - top, y: rect.minY))
        addArc(
            center: CGPoint(x: rect.maxX - top, y: rect.minY + top),
            radius: top,
            startAngle: .degrees(270),
            endAngle: .degrees(0),
            clockwise: false
        )
        addLine(to: CGPoint(x: rect.maxX, y: bodyBottom - bottom))
        addArc(
            center: CGPoint(x: rect.maxX - bottom, y: bodyBottom - bottom),
            radius: bottom,
            startAngle: .degrees(0),
            endAngle: .degrees(90),
            clockwise: false
        )
    }

    /// Draws the lobe from the body's right edge round to its left, joined by the fillet. A zero
    /// fillet gives a plain half circle.
    mutating func addLobe(in rect: CGRect, lobe: CGFloat, fillet: CGFloat, bodyBottom: CGFloat) {
        guard fillet > 0, lobe > 0 else {
            addLine(to: CGPoint(x: rect.midX + lobe, y: bodyBottom))
            addArc(
                center: CGPoint(x: rect.midX, y: bodyBottom),
                radius: lobe,
                startAngle: .degrees(0),
                endAngle: .degrees(180),
                clockwise: false
            )
            return
        }

        let reach = sqrt(lobe * lobe + 2 * lobe * fillet)
        let gamma = Angle.radians(atan2(fillet, reach))

        addLine(to: CGPoint(x: rect.midX + reach, y: bodyBottom))
        addArc(
            center: CGPoint(x: rect.midX + reach, y: bodyBottom + fillet),
            radius: fillet,
            startAngle: .degrees(270),
            endAngle: .degrees(180) + gamma,
            clockwise: true
        )
        addArc(
            center: CGPoint(x: rect.midX, y: bodyBottom),
            radius: lobe,
            startAngle: gamma,
            endAngle: .degrees(180) - gamma,
            clockwise: false
        )
        addArc(
            center: CGPoint(x: rect.midX - reach, y: bodyBottom + fillet),
            radius: fillet,
            startAngle: .zero - gamma,
            endAngle: .degrees(270),
            clockwise: true
        )
    }
}
