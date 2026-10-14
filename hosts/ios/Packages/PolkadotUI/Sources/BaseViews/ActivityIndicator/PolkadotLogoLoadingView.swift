import UIKit
import DesignSystem

/// Loading indicator drawn as the six-petal Polkadot mark with a highlight walking around it.
/// Animations are re-installed whenever the view lands in a window or the app returns to the
/// foreground, so `startAnimating()` is safe to call before the view is in the hierarchy.
public final class PolkadotLogoLoadingView: UIView, LoadIndicatorRepresentable {
    public private(set) var isAnimating = false

    /// Edge of the square the mark is fitted into. Drives the intrinsic size; when constraints
    /// impose different bounds the mark scales to fit them instead.
    public var size: CGFloat {
        didSet {
            guard size != oldValue else { return }
            invalidateIntrinsicContentSize()
            setNeedsLayout()
        }
    }

    private let markLayer = CALayer()
    private var petalLayers: [CALayer] = []
    private var foregroundObserver: NSObjectProtocol?

    public init(size: CGFloat = Constants.defaultSize, color: UIColor = .fgPrimary) {
        self.size = size
        super.init(frame: .zero)
        setupLayers(color: color)
        setupForegroundObserver()
        alpha = 0
        backgroundColor = .clear
        isAccessibilityElement = false
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        if let foregroundObserver {
            NotificationCenter.default.removeObserver(foregroundObserver)
        }
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: size, height: size)
    }

    override public func layoutSubviews() {
        super.layoutSubviews()
        layoutMark()
    }

    override public func didMoveToWindow() {
        super.didMoveToWindow()
        guard window != nil, isAnimating else { return }
        installAnimations()
    }
}

// MARK: - Public

public extension PolkadotLogoLoadingView {
    enum Constants {
        public static let defaultSize: CGFloat = 96
    }

    func startAnimating() {
        isAnimating = true
        UIView.animate(withDuration: Constants.fadeDuration) { [self] in
            alpha = 1
        }
        installAnimations()
    }

    func stopAnimating() {
        isAnimating = false
        UIView.animate(withDuration: Constants.fadeDuration) { [self] in
            alpha = 0
        }
        removeAnimations()
    }
}

// MARK: - Private

private extension PolkadotLogoLoadingView {
    enum AnimationKey {
        static let opacity = "petal.opacity"
        static let scale = "petal.scale"
    }

    func setupLayers(color: UIColor) {
        markLayer.bounds = CGRect(origin: .zero, size: PolkadotLogoMark.viewBox)
        layer.addSublayer(markLayer)

        petalLayers = PolkadotLogoMark.petals.map { petal in
            let container = CALayer()
            container.bounds = markLayer.bounds
            container.position = CGPoint(x: markLayer.bounds.midX, y: markLayer.bounds.midY)
            container.opacity = Float(PolkadotLogoLoadingAnimation.restingOpacity)
            container.addSublayer(makeShape(path: petal.fill, fill: color, stroke: nil))
            container.addSublayer(makeShape(
                path: petal.stroke,
                fill: nil,
                stroke: color.withAlphaComponent(Constants.strokeAlpha)
            ))
            markLayer.addSublayer(container)
            return container
        }
    }

    func makeShape(path: UIBezierPath, fill: UIColor?, stroke: UIColor?) -> CAShapeLayer {
        let shape = CAShapeLayer()
        shape.path = path.cgPath
        shape.fillColor = fill?.cgColor
        shape.strokeColor = stroke?.cgColor
        shape.lineWidth = stroke == nil ? 0 : Constants.strokeWidth
        return shape
    }

    func layoutMark() {
        let box = PolkadotLogoMark.viewBox
        guard bounds.width > 0, bounds.height > 0 else { return }
        let scale = min(bounds.width / box.width, bounds.height / box.height)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        markLayer.position = CGPoint(x: bounds.midX, y: bounds.midY)
        markLayer.transform = CATransform3DMakeScale(scale, scale, 1)
        CATransaction.commit()
    }

    func setupForegroundObserver() {
        foregroundObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.willEnterForegroundNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.isAnimating else { return }
                self.installAnimations()
            }
        }
    }

    func installAnimations() {
        removeAnimations()

        guard !UIAccessibility.isReduceMotionEnabled else { return }

        for (index, petal) in petalLayers.enumerated() {
            let keyframes = PolkadotLogoLoadingAnimation.keyframes(index: index)
            petal.add(
                makeKeyframeAnimation(keyPath: "opacity", values: keyframes.opacity),
                forKey: AnimationKey.opacity
            )
            petal.add(
                makeKeyframeAnimation(keyPath: "transform.scale", values: keyframes.scale),
                forKey: AnimationKey.scale
            )
        }
    }

    func removeAnimations() {
        for petal in petalLayers {
            petal.removeAnimation(forKey: AnimationKey.opacity)
            petal.removeAnimation(forKey: AnimationKey.scale)
        }
    }

    func makeKeyframeAnimation(keyPath: String, values: [Double]) -> CAKeyframeAnimation {
        let animation = CAKeyframeAnimation(keyPath: keyPath)
        animation.values = values
        animation.calculationMode = .linear
        animation.duration = PolkadotLogoLoadingAnimation.cycle
        animation.repeatCount = .infinity
        return animation
    }
}

private extension PolkadotLogoLoadingView.Constants {
    static let fadeDuration: TimeInterval = 0.25
    static let strokeAlpha: CGFloat = 0.12
    static let strokeWidth: CGFloat = 1
}
