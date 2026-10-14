import UIKit

public enum DSTabBarConnectionGlyph {
    public static let image: UIImage = render()
}

private extension DSTabBarConnectionGlyph {
    static let discs: [(diameter: CGFloat, opacity: CGFloat)] = [
        (0.8571, 0.3),
        (0.5714, 0.6),
        (0.2857, 1)
    ]

    static func render() -> UIImage {
        let side = DSTabBarMetrics.iconSize
        let center = CGPoint(x: side / 2, y: side / 2)

        let renderer = UIGraphicsImageRenderer(size: CGSize(width: side, height: side))
        let image = renderer.image { _ in
            discs.forEach { disc in
                let radius = side * disc.diameter / 2
                let path = UIBezierPath(
                    arcCenter: center,
                    radius: radius,
                    startAngle: 0,
                    endAngle: 2 * .pi,
                    clockwise: true
                )
                UIColor.black.withAlphaComponent(disc.opacity).setFill()
                path.fill(with: .copy, alpha: 1)
            }
        }

        return image.withRenderingMode(.alwaysTemplate)
    }
}
