import PolkadotUI
import SnapKit
import UIKit

/// The Docklet tip beside the Edge glow. It sits in the chrome rather than in a system popover, so a drag can fling it
/// away.
final class DockletTipView: UIView {
    /// Gap between the tip and the Edge glow. It keeps the tip clear of the Grab zone, which starts further in.
    private static let gapToGlow: CGFloat = 24
    private static let width: CGFloat = 240
    private static let cornerRadius: CGFloat = 16
    private static let padding: CGFloat = 12
    private static let textSpacing: CGFloat = 4
    /// Horizontal offset the tip slides in from.
    private static let slideInOffset: CGFloat = 24
    /// Horizontal distance the tip flies over as it leaves.
    private static let flyOffDistance: CGFloat = 480
    private static let appearDuration: TimeInterval = 0.9
    private static let flyOffDuration: TimeInterval = 0.32
    private static let snapBackDuration: TimeInterval = 0.25
    /// Control points of the curve the fly-off follows.
    private static let flyOffControlPoint1 = CGPoint(x: 0.25, y: 0.8)
    private static let flyOffControlPoint2 = CGPoint(x: 0.3, y: 1)

    /// Called when the tip is tapped.
    var onTap: (() -> Void)?
    /// Called when a drag ends, with the horizontal distance it covered.
    var onDragEnded: ((CGFloat) -> Void)?

    private let titleLabel = UILabel()
    private let messageLabel = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows the tip beside `glowFrame`, sliding in and fading up.
    func present(title: String, message: String, beside glowFrame: CGRect) {
        titleLabel.text = title
        messageLabel.text = message
        let fittedSize = systemLayoutSizeFitting(
            CGSize(width: Self.width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        )
        frame = CGRect(
            x: glowFrame.minX - Self.gapToGlow - Self.width,
            y: glowFrame.midY - fittedSize.height / 2,
            width: Self.width,
            height: fittedSize.height
        )
        transform = CGAffineTransform(translationX: Self.slideInOffset, y: 0)
        alpha = 0
        isHidden = false
        isUserInteractionEnabled = true
        UIView.animate(withDuration: Self.appearDuration, delay: 0, options: .curveEaseOut) {
            self.transform = .identity
            self.alpha = 1
        }
    }

    /// Flies the tip off in the direction of `translationX`, then hides it. A zero distance flies it to the right.
    func flyOff(translationX: CGFloat) {
        let direction: CGFloat = translationX < 0 ? -1 : 1
        isUserInteractionEnabled = false
        let animator = UIViewPropertyAnimator(
            duration: Self.flyOffDuration,
            timingParameters: UICubicTimingParameters(
                controlPoint1: Self.flyOffControlPoint1,
                controlPoint2: Self.flyOffControlPoint2
            )
        )
        animator.addAnimations {
            self.transform = CGAffineTransform(translationX: direction * Self.flyOffDistance, y: 0)
            self.alpha = 0
        }
        animator.addCompletion { [weak self] _ in
            self?.isHidden = true
        }
        animator.startAnimation()
    }

    /// Returns the tip to its rest after a drag that did not go far enough to restore the bar.
    func snapBack() {
        UIView.animate(withDuration: Self.snapBackDuration) {
            self.transform = .identity
        }
    }
}

private extension DockletTipView {
    func setupViews() {
        backgroundColor = .fgPrimary
        layer.cornerRadius = Self.cornerRadius
        isHidden = true

        titleLabel.font = .preferredFont(forTextStyle: .headline)
        titleLabel.textColor = .fgPrimaryInverted
        titleLabel.numberOfLines = 0
        messageLabel.font = .preferredFont(forTextStyle: .subheadline)
        messageLabel.textColor = .fgPrimaryInverted
        messageLabel.numberOfLines = 0

        let textStack = UIStackView(arrangedSubviews: [titleLabel, messageLabel])
        textStack.axis = .vertical
        textStack.spacing = Self.textSpacing
        addSubview(textStack)
        textStack.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(Self.padding)
        }

        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(handleTap)))
        addGestureRecognizer(UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:))))
    }

    @objc func handleTap() {
        onTap?()
    }

    @objc func handlePan(_ recognizer: UIPanGestureRecognizer) {
        let translationX = recognizer.translation(in: superview).x
        switch recognizer.state {
        case .changed:
            transform = CGAffineTransform(translationX: translationX, y: 0)
        case .ended,
             .cancelled:
            onDragEnded?(translationX)
        default:
            break
        }
    }
}
