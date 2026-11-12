import PolkadotUI
import UIKit
import UIKitExt

/// The pull-down every opened card closes with. It starts only on the card, never during a transition, and the card
/// follows the finger. The body below it moves with the card, fades and blurs up to `PocketClosePull.maxBlur`.
/// Past the collapse distance it closes, with the flight starting where the card was left. Short of it, the card
/// springs back. The pan runs alongside the scroll pans, so iOS 17 keeps it. The body is a snapshot, since a
/// Pocket card's body is a web view that cannot follow the finger itself.
@MainActor
final class PocketPullDown: NSObject, UIGestureRecognizerDelegate {
    private static let springDuration: TimeInterval = 0.45

    private weak var screen: PocketFlightScreen?
    private let close: () -> Void
    private lazy var pan = UIPanGestureRecognizer(target: self, action: #selector(followPull))
    private var overlay: PullOverlay?

    init(screen: PocketFlightScreen, close: @escaping () -> Void) {
        self.screen = screen
        self.close = close
    }

    var isPulling: Bool {
        pan.state == .began || pan.state == .changed
    }

    func install() {
        pan.delegate = self
        screen?.view.addGestureRecognizer(pan)
    }

    func gestureRecognizerShouldBegin(_: UIGestureRecognizer) -> Bool {
        guard let screen, screen.transitionCoordinator == nil else { return false }

        let velocity = pan.velocity(in: screen.view)
        let card = screen.itemFrame(in: screen.view)
        return card.contains(pan.location(in: screen.view)) && velocity.y > abs(velocity.x)
    }

    func gestureRecognizer(_: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith _: UIGestureRecognizer) -> Bool {
        true
    }
}

/// The views a pull moves: the card, and the body under it with its blurred copy above the sharp one.
private struct PullOverlay {
    let view: UIView
    let card: UIView
    let body: UIView
    let blurredBody: UIView
    let startCard: CGRect
}

private extension PocketPullDown {
    @objc func followPull(_ gesture: UIPanGestureRecognizer) {
        guard let screen else { return }

        let travel = gesture.translation(in: screen.view).y
        let pull = PocketClosePull(travel: travel, screenHeight: screen.view.bounds.height)
        switch gesture.state {
        case .began:
            beginPull(on: screen)
        case .changed:
            show(pull)
        case .ended:
            guard overlay != nil else { return }

            if pull.closes {
                closeCard(draggedBy: travel)
            } else {
                springBack()
            }
        case .cancelled,
             .failed:
            springBack()
        default:
            break
        }
    }

    /// Snapshots are taken before the overlay goes in, so the overlay never appears in its own images.
    func beginPull(on screen: PocketFlightScreen) {
        overlay?.view.removeFromSuperview()
        overlay = nil
        guard let pullOverlay = makeOverlay(of: screen.view, card: screen.itemFrame(in: screen.view)) else { return }

        screen.view.addSubview(pullOverlay.view)
        overlay = pullOverlay
    }

    func show(_ pull: PocketClosePull) {
        guard let overlay else { return }

        let drop = CGAffineTransform(translationX: 0, y: max(pull.travel, 0))
        overlay.card.transform = drop
        overlay.body.transform = drop
        overlay.body.alpha = pull.bodyAlpha
        overlay.blurredBody.alpha = pull.progress
    }

    /// The card's flight starts where the snapshot card is, so the real card is moved to that spot,
    /// wherever the scroll underneath has left it.
    func closeCard(draggedBy travel: CGFloat) {
        if let overlay, let screen {
            let realCard = screen.itemFrame(in: screen.view)
            screen.moveItem(by: CGPoint(x: 0, y: overlay.startCard.minY + max(travel, 0) - realCard.minY))
        }
        overlay?.view.removeFromSuperview()
        overlay = nil
        close()
    }

    func springBack() {
        guard let overlay else { return }

        let animator = UIViewPropertyAnimator(duration: Self.springDuration, dampingRatio: 0.85) {
            overlay.card.transform = .identity
            overlay.body.transform = .identity
            overlay.body.alpha = 1
            overlay.blurredBody.alpha = 0
        }
        animator.addCompletion { _ in overlay.view.removeFromSuperview() }
        animator.startAnimation()
    }

    func makeOverlay(of screenView: UIView, card cardFrame: CGRect) -> PullOverlay? {
        let bodyFrame = CGRect(
            x: 0,
            y: cardFrame.maxY,
            width: screenView.bounds.width,
            height: screenView.bounds.maxY - cardFrame.maxY
        )
        guard
            let bodyImage = screenView.snapshot(of: bodyFrame),
            let cardImage = screenView.snapshot(of: cardFrame),
            let blurredImage = bodyImage.blurred(radius: PocketClosePull.maxBlur)
        else { return nil }

        let view = UIView(frame: screenView.bounds)
        view.backgroundColor = .bgSurfaceMain
        let card = imageView(cardImage, frame: cardFrame)
        let body = UIView(frame: bodyFrame)
        let sharpBody = imageView(bodyImage, frame: body.bounds)
        let blurredBody = imageView(blurredImage, frame: body.bounds)
        blurredBody.alpha = 0
        body.addSubview(sharpBody)
        body.addSubview(blurredBody)
        view.addSubview(body)
        view.addSubview(card)

        return PullOverlay(view: view, card: card, body: body, blurredBody: blurredBody, startCard: cardFrame)
    }

    func imageView(_ image: UIImage, frame: CGRect) -> UIImageView {
        let view = UIImageView(image: image)
        view.frame = frame
        return view
    }
}
