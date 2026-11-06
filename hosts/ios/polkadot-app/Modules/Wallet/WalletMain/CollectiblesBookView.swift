import DesignSystem
import PolkadotUI
import UIKit

/// The timing and sizes of the Collectibles book, shared by the Pocket peek, the book's flight and its opening.
enum CollectiblesBookTiming {
    static let pressedScale: CGFloat = 0.98
    static let pressDuration: TimeInterval = 0.08
    static let colourDuration: TimeInterval = 0.17
    static let backdropFade: TimeInterval = 0.18
    static let flightStiffness: CGFloat = 260
    static let liftDuration: TimeInterval = 0.4
    static let liftTilt: CGFloat = -4 * .pi / 180
    static let coverAngle: CGFloat = -160 * .pi / 180
    static let coverDuration: TimeInterval = 0.22
    static let coverCameraDistance: CGFloat = 2_304
    static let pageExpandDuration: TimeInterval = 0.25
    static let pageCornerRadius: CGFloat = 20
    static let pageInset: CGFloat = DSSpacings.mediumIncreased
    static let loadTimeout: TimeInterval = 10
    static let settleCap: TimeInterval = 1
}

/// The Collectibles book cover, drawn over the page until it swings open. It is the Pocket peek's own picture, so the
/// book starts exactly where the peek sits.
final class CollectiblesBookView: UIView {
    private let tiltView = UIView()
    private let cover = UIImageView(image: UIImage.collectiblesBook)

    override init(frame: CGRect) {
        super.init(frame: frame)

        tiltView.frame = bounds
        tiltView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(tiltView)

        cover.contentMode = .top
        cover.clipsToBounds = true
        cover.layer.anchorPoint = CGPoint(x: 0, y: 0.5)
        tiltView.addSubview(cover)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        cover.bounds = CGRect(origin: .zero, size: bounds.size)
        cover.center = CGPoint(x: 0, y: bounds.midY)
    }

    /// Tilts the book and levels it again, before its cover opens.
    func lift(then completion: @escaping () -> Void) {
        UIView.animateKeyframes(
            withDuration: CollectiblesBookTiming.liftDuration,
            delay: 0,
            options: [],
            animations: {
                UIView.addKeyframe(withRelativeStartTime: 0, relativeDuration: 0.5) {
                    self.tiltView.transform = CGAffineTransform(rotationAngle: CollectiblesBookTiming.liftTilt)
                }
                UIView.addKeyframe(withRelativeStartTime: 0.5, relativeDuration: 0.5) {
                    self.tiltView.transform = .identity
                }
            },
            completion: { _ in completion() }
        )
    }

    /// Swings the cover open on its left edge, then hides the book.
    func openCover(then completion: @escaping () -> Void) {
        tiltView.layer.sublayerTransform.m34 = -1 / CollectiblesBookTiming.coverCameraDistance
        cover.layer.isDoubleSided = false
        cover.layer.transform = CATransform3DMakeRotation(CollectiblesBookTiming.coverAngle, 0, 1, 0)

        let swing = CABasicAnimation(keyPath: "transform.rotation.y")
        swing.fromValue = 0
        swing.toValue = CollectiblesBookTiming.coverAngle
        swing.duration = CollectiblesBookTiming.coverDuration
        swing.timingFunction = CAMediaTimingFunction(name: .easeIn)

        CATransaction.begin()
        CATransaction.setCompletionBlock { [self] in
            cover.isHidden = true
            isHidden = true
            completion()
        }
        cover.layer.add(swing, forKey: "cover")
        CATransaction.commit()
    }

    /// Moves the whole book by `offset` from where it sits. Zero puts it back in place.
    func setFlightOffset(_ offset: CGPoint) {
        transform = CGAffineTransform(translationX: offset.x, y: offset.y)
    }
}
