import UIKit

/// The Pocket a card is opened from: where the card sits in it, and which card
/// it should stop drawing while that card's screen draws it.
@MainActor
protocol PocketCardOrigin: AnyObject {
    /// The card's frame in window coordinates, or nil when the Pocket cannot
    /// show the card where the user would see it land.
    func frame(of key: PocketCardKey) -> CGRect?

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
    /// In window coordinates.
    private let slot: CGRect
    private let opening: Bool

    init(screen: PocketCardScreenViewController, slot: CGRect, opening: Bool) {
        self.screen = screen
        self.slot = slot
        self.opening = opening
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        Self.duration
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        let backdrop = addViews(for: context)

        screen.beginFlight()
        container.layoutIfNeeded()

        let card = screen.cardFrame(in: container)
        let landing = container.convert(slot, from: nil)
        let toSlot = CGPoint(x: landing.minX - card.minX, y: landing.minY - card.minY)
        let surroundingsAlpha: CGFloat = opening ? 1 : 0

        if opening {
            screen.moveCard(by: toSlot)
            screen.setSurroundingsAlpha(0)
            backdrop.alpha = 0
        }

        let animator = UIViewPropertyAnimator(duration: Self.duration, timingParameters: Self.timing)
        animator.addAnimations { [self] in
            screen.moveCard(by: opening ? .zero : toSlot)
            screen.setSurroundingsAlpha(surroundingsAlpha)
            backdrop.alpha = surroundingsAlpha
        }
        animator.addCompletion { [self] _ in
            if opening {
                finish(context, removing: backdrop)
            } else {
                land(context, removing: backdrop)
            }
        }
        animator.startAnimation()
    }
}

private extension PocketCardFlight {
    var presented: UIViewController {
        screen.navigationController ?? screen
    }

    /// The backdrop fades in for the Pocket to fade out behind the card.
    func addViews(for context: UIViewControllerContextTransitioning) -> UIView {
        let container = context.containerView
        let backdrop = UIView(frame: container.bounds)
        backdrop.backgroundColor = .bgSurfaceMain

        if opening {
            presented.view.frame = context.finalFrame(for: presented)
            container.addSubview(backdrop)
            container.addSubview(presented.view)
        } else {
            container.insertSubview(backdrop, at: 0)
            if let pocket = context.view(forKey: .to) {
                pocket.frame = container.bounds
                container.insertSubview(pocket, at: 0)
            }
        }

        return backdrop
    }

    func land(_ context: UIViewControllerContextTransitioning, removing backdrop: UIView) {
        screen.showCardInPocket()
        UIView.animate(
            withDuration: Self.landingFade,
            animations: { self.presented.view.alpha = 0 },
            completion: { _ in self.finish(context, removing: backdrop) }
        )
    }

    func finish(_ context: UIViewControllerContextTransitioning, removing backdrop: UIView) {
        backdrop.removeFromSuperview()
        screen.endFlight()
        context.completeTransition(!context.transitionWasCancelled)
    }
}
