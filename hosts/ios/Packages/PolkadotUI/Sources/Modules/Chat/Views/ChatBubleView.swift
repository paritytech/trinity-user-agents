import UIKit

public final class ChatBubbleView: UIView {
    public var corners: CornersConfiguration = .zero {
        didSet {
            guard oldValue != corners else { return }
            setNeedsLayout()
        }
    }

    public var fillColor: UIColor = .clear {
        didSet {
            backgroundColor = fillColor
        }
    }

    public var strokeColor: UIColor? {
        didSet { applyStroke() }
    }

    public var strokeWidth: CGFloat = 0 {
        didSet { applyStroke() }
    }

    private let bubbleMaskLayer = CAShapeLayer()
    private let strokeLayer: CAShapeLayer = {
        let layer = CAShapeLayer()
        layer.fillColor = UIColor.clear.cgColor
        return layer
    }()

    private var lastBounds: CGRect = .null
    private var lastCorners: CornersConfiguration = .zero

    public init(corners: CornersConfiguration = .zero, fillColor: UIColor = .clear) {
        self.corners = corners
        self.fillColor = fillColor
        super.init(frame: .zero)
        commonInit()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    private func commonInit() {
        isOpaque = false
        backgroundColor = fillColor
        layer.mask = bubbleMaskLayer
        layer.addSublayer(strokeLayer)
    }

    override public func layoutSubviews() {
        super.layoutSubviews()

        guard bounds != lastBounds || corners != lastCorners else { return }
        lastBounds = bounds
        lastCorners = corners

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let path = UIBezierPath(
            roundedRect: bounds,
            configuration: corners
        ).cgPath
        bubbleMaskLayer.frame = bounds
        bubbleMaskLayer.path = path
        strokeLayer.frame = bounds
        strokeLayer.path = path
        CATransaction.commit()
    }

    private func applyStroke() {
        strokeLayer.strokeColor = strokeColor?.cgColor
        strokeLayer.lineWidth = strokeColor == nil ? 0 : strokeWidth * 2
    }
}

#Preview(traits: .fixedLayout(width: 100, height: 50)) {
    ChatBubbleView(corners: .all(16), fillColor: .systemBlue)
}
