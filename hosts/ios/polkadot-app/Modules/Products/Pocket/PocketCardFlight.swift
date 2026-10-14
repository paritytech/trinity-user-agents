import UIKit

/// The Pocket a card is opened from: where the card sits in it, and which card
/// it should stop drawing while that card's screen draws it.
@MainActor
protocol PocketCardOrigin: AnyObject {
    /// The card's frame in window coordinates, or nil when the Pocket cannot
    /// show the card where the user would see it land.
    func frame(of key: PocketCardKey) -> CGRect?

    /// Hides the card `key` names, or none.
    func setOpenCard(_ key: PocketCardKey?)
}

/// Moves an opened card between its place in the Pocket and the top of its
/// screen, while the rest of the screen fades in or out around it, so the user
/// sees one card move as on Android.
final class PocketCardFlight: NSObject, UIViewControllerAnimatedTransitioning {
    private static let duration: TimeInterval = 0.5
    /// Android's curve, which carries the card slightly past its place.
    private static let timing = UICubicTimingParameters(
        controlPoint1: CGPoint(x: 0.34, y: 1.2),
        controlPoint2: CGPoint(x: 0.64, y: 1)
    )
    /// Most of a card in the Pocket sits under the next one, so the landed card
    /// fades into the Pocket's own instead of snapping behind its neighbour.
    private static let landingFade: TimeInterval = 0.12

    private let screen: PocketCardScreenViewController
    private let origin: PocketCardOrigin
    private let key: PocketCardKey
    private let opening: Bool

    init(screen: PocketCardScreenViewController, origin: PocketCardOrigin, key: PocketCardKey, opening: Bool) {
        self.screen = screen
        self.origin = origin
        self.key = key
        self.opening = opening
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        Self.duration
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        guard
            let presented = context.viewController(forKey: opening ? .to : .from),
            let underneath = context.viewController(forKey: opening ? .from : .to)
        else {
            context.completeTransition(!context.transitionWasCancelled)
            return
        }

        let container = context.containerView
        let backdrop = UIView(frame: container.bounds)
        backdrop.backgroundColor = .bgSurfaceMain

        if opening {
            presented.view.frame = context.finalFrame(for: presented)
            container.addSubview(backdrop)
            container.addSubview(presented.view)
        } else {
            underneath.view.frame = context.finalFrame(for: underneath)
            container.insertSubview(underneath.view, at: 0)
            container.insertSubview(backdrop, aboveSubview: underneath.view)
        }

        screen.beginFlight()
        container.layoutIfNeeded()

        let card = screen.cardFrame(in: container)
        let slot = origin.frame(of: key).map { container.convert($0, from: nil) } ?? card
        let toSlot = CGPoint(x: slot.minX - card.minX, y: slot.minY - card.minY)
        let shown: CGFloat = opening ? 1 : 0

        if opening {
            origin.setOpenCard(key)
            screen.moveCard(by: toSlot)
            screen.setSurroundingsAlpha(0)
            backdrop.alpha = 0
        }

        let animator = UIViewPropertyAnimator(duration: Self.duration, timingParameters: Self.timing)
        animator.addAnimations { [self] in
            screen.moveCard(by: opening ? .zero : toSlot)
            screen.setSurroundingsAlpha(shown)
            backdrop.alpha = shown
        }
        animator.addCompletion { [self] _ in
            let finish = {
                backdrop.removeFromSuperview()
                self.screen.endFlight()
                context.completeTransition(!context.transitionWasCancelled)
            }

            guard !opening else { return finish() }

            origin.setOpenCard(nil)
            UIView.animate(
                withDuration: Self.landingFade,
                animations: { presented.view.alpha = 0 },
                completion: { _ in finish() }
            )
        }
        animator.startAnimation()
    }
}
