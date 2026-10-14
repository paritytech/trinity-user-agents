import Foundation
import UIKit

extension UIBezierPath {
    convenience init(
        roundedRect rect: CGRect,
        configuration: CornersConfiguration
    ) {
        if configuration.allEqual {
            // Native continuous corners (squircle)
            self.init(roundedRect: rect, cornerRadius: configuration.topLeft)
        } else {
            self.init(roundedRect: rect, cornerSpecs: [
                (.layerMinXMinYCorner, configuration.topLeft),
                (.layerMaxXMinYCorner, configuration.topRight),
                (.layerMaxXMaxYCorner, configuration.bottomRight),
                (.layerMinXMaxYCorner, configuration.bottomLeft),
            ])
        }
    }

    // Fallback for different radii per corner (circular arcs, not continuous)
    convenience init(
        roundedRect rect: CGRect,
        cornerSpecs: [(CACornerMask, CGFloat)]
    ) {
        self.init()

        var topLeftInput: CGFloat = 0
        var topRightInput: CGFloat = 0
        var bottomRightInput: CGFloat = 0
        var bottomLeftInput: CGFloat = 0

        for (cornerMask, radius) in cornerSpecs {
            if cornerMask.contains(.layerMinXMinYCorner) {
                topLeftInput = radius
            }
            if cornerMask.contains(.layerMaxXMinYCorner) {
                topRightInput = radius
            }
            if cornerMask.contains(.layerMaxXMaxYCorner) {
                bottomRightInput = radius
            }
            if cornerMask.contains(.layerMinXMaxYCorner) {
                bottomLeftInput = radius
            }
        }

        let maxRadiusWidth = rect.width / 2
        let maxRadiusHeight = rect.height / 2

        let topLeftRadius = min(topLeftInput, maxRadiusWidth, maxRadiusHeight)
        let topRightRadius = min(topRightInput, maxRadiusWidth, maxRadiusHeight)
        let bottomRightRadius = min(bottomRightInput, maxRadiusWidth, maxRadiusHeight)
        let bottomLeftRadius = min(bottomLeftInput, maxRadiusWidth, maxRadiusHeight)

        move(to: CGPoint(x: rect.minX + topLeftRadius, y: rect.minY))

        addLine(to: CGPoint(x: rect.maxX - topRightRadius, y: rect.minY))
        if topRightRadius > 0 {
            addArc(
                withCenter: CGPoint(
                    x: rect.maxX - topRightRadius,
                    y: rect.minY + topRightRadius
                ),
                radius: topRightRadius,
                startAngle: -.pi / 2,
                endAngle: 0,
                clockwise: true
            )
        } else {
            addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        }

        addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - bottomRightRadius))
        if bottomRightRadius > 0 {
            addArc(
                withCenter: CGPoint(
                    x: rect.maxX - bottomRightRadius,
                    y: rect.maxY - bottomRightRadius
                ),
                radius: bottomRightRadius,
                startAngle: 0,
                endAngle: .pi / 2,
                clockwise: true
            )
        } else {
            addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        }

        addLine(to: CGPoint(x: rect.minX + bottomLeftRadius, y: rect.maxY))
        if bottomLeftRadius > 0 {
            addArc(
                withCenter: CGPoint(
                    x: rect.minX + bottomLeftRadius,
                    y: rect.maxY - bottomLeftRadius
                ),
                radius: bottomLeftRadius,
                startAngle: .pi / 2,
                endAngle: .pi,
                clockwise: true
            )
        } else {
            addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        }

        addLine(to: CGPoint(x: rect.minX, y: rect.minY + topLeftRadius))
        if topLeftRadius > 0 {
            addArc(
                withCenter: CGPoint(
                    x: rect.minX + topLeftRadius,
                    y: rect.minY + topLeftRadius
                ),
                radius: topLeftRadius,
                startAngle: .pi,
                endAngle: 3 * .pi / 2,
                clockwise: true
            )
        } else {
            addLine(to: CGPoint(x: rect.minX, y: rect.minY))
        }

        close()
    }
}
