import UIKit
import DesignSystem

final class DSTabBarTabsGlyphView: UIView {
    var count: Int = 0 {
        didSet {
            guard count != oldValue else { return }
            countLabel.text = "\(count)"
        }
    }

    var glyphColor: UIColor = .fgSecondary {
        didSet {
            guard glyphColor != oldValue else { return }
            applyColor()
        }
    }

    private let backLayer = CAShapeLayer()
    private let frontLayer = CAShapeLayer()
    private let countLabel = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)

        isUserInteractionEnabled = false

        [backLayer, frontLayer].forEach { shape in
            shape.fillColor = nil
            shape.lineCap = .round
            shape.lineJoin = .round
            layer.addSublayer(shape)
        }

        countLabel.font = .labelSmallEmphasized
        countLabel.textAlignment = .center
        addSubview(countLabel)

        applyColor()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func refreshColorsForTraitChange() {
        applyColor()
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        let side = min(bounds.width, bounds.height)
        let cardRect = frontCardRect(side: side)

        [backLayer, frontLayer].forEach {
            $0.frame = bounds
            $0.lineWidth = side * Ratio.stroke
        }
        frontLayer.path = UIBezierPath(
            roundedRect: cardRect,
            cornerRadius: side * Ratio.frontCornerRadius
        ).cgPath
        backLayer.path = backCardPath(side: side).cgPath

        countLabel.frame = cardRect
    }
}

private extension DSTabBarTabsGlyphView {
    enum Ratio {
        static let stroke: CGFloat = 0.1094
        static let cardSide: CGFloat = 0.7082
        static let frontCornerRadius: CGFloat = 0.1094
        static let backCornerRadius: CGFloat = 0.2552
        static let backVerticalEnd: CGFloat = 0.6962
        static let backHorizontalEnd: CGFloat = 0.2518

        static var inset: CGFloat { stroke / 2 }
    }

    func frontCardRect(side: CGFloat) -> CGRect {
        CGRect(
            x: side * Ratio.inset,
            y: side * (1 - Ratio.inset - Ratio.cardSide),
            width: side * Ratio.cardSide,
            height: side * Ratio.cardSide
        )
    }

    func backCardPath(side: CGFloat) -> UIBezierPath {
        let right = side * (1 - Ratio.inset)
        let top = side * Ratio.inset
        let radius = side * Ratio.backCornerRadius

        let path = UIBezierPath()
        path.move(to: CGPoint(x: right, y: side * Ratio.backVerticalEnd))
        path.addLine(to: CGPoint(x: right, y: top + radius))
        path.addArc(
            withCenter: CGPoint(x: right - radius, y: top + radius),
            radius: radius,
            startAngle: 0,
            endAngle: -.pi / 2,
            clockwise: false
        )
        path.addLine(to: CGPoint(x: side * Ratio.backHorizontalEnd, y: top))
        return path
    }

    func applyColor() {
        backLayer.strokeColor = glyphColor.cgColor
        frontLayer.strokeColor = glyphColor.cgColor
        countLabel.textColor = glyphColor
    }
}
