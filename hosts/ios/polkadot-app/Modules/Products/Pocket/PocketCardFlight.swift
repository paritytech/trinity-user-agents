import SwiftUI
import UIKit

/// The Pocket a card is opened from: where the card sits in it, and how it
/// draws that card while the card's screen is up.
@MainActor
protocol PocketCardOrigin: AnyObject {
    /// The card's frame in window coordinates, or nil when the Pocket cannot
    /// show the card where the user would see it land.
    func frame(of key: PocketCardKey) -> CGRect?

    /// The card whose screen is up, or nil once it has closed.
    func setOpenCard(_ key: PocketCardKey?)

    /// Draws the open card `offset` away from its slot, and clears the rest of
    /// the Pocket away from it or brings it back, on the flight's clock when
    /// animated.
    func moveOpenCard(by offset: CGSize, clearingPocket: Bool, animated: Bool)
}

/// One clock for everything a flight moves, in UIKit and in SwiftUI alike, so
/// the card, its screen and the Pocket stay in step.
enum PocketCardFlightTiming {
    static let duration: TimeInterval = 0.5

    /// Android's curve, which carries the card slightly past its place.
    private static let curveStart = CGPoint(x: 0.34, y: 1.2)
    private static let curveEnd = CGPoint(x: 0.64, y: 1)

    static var animation: Animation {
        .timingCurve(curveStart.x, curveStart.y, curveEnd.x, curveEnd.y, duration: duration)
    }

    static func makeAnimator() -> UIViewPropertyAnimator {
        UIViewPropertyAnimator(
            duration: duration,
            timingParameters: UICubicTimingParameters(controlPoint1: curveStart, controlPoint2: curveEnd)
        )
    }
}

/// Carries an opened card between its place in the Pocket and the top of its
/// screen. The Pocket draws the card while it moves, so the card keeps its depth
/// in the stack; the screen fades its bar, carries its page under the card, and
/// draws the card itself only at rest. A card the Pocket cannot place when the
/// flight starts fades with the rest of the screen: the Pocket may no longer be
/// what the screen opens from or closes to.
final class PocketCardFlight: NSObject, UIViewControllerAnimatedTransitioning {
    /// The page is shown only once the Pocket's cards have left it, and is gone
    /// before they come back over it.
    private static let pageDelay: CGFloat = 0.35
    private static let pageFadeOut = PocketCardFlightTiming.duration * 0.2

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
        PocketCardFlightTiming.duration
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        // The Pocket stays in the window under the card, so a close adds nothing.
        if opening {
            presented.view.frame = context.finalFrame(for: presented)
            container.addSubview(presented.view)
        }

        screen.beginFlight()
        container.layoutIfNeeded()

        let toSlot = origin.frame(of: key).map { slot in
            let card = screen.cardFrame(in: container)
            let landing = container.convert(slot, from: nil)
            return CGPoint(x: landing.minX - card.minX, y: landing.minY - card.minY)
        }

        if let toSlot {
            fly(context, toSlot: toSlot)
        } else {
            fade(context)
        }
    }
}

private extension PocketCardFlight {
    var presented: UIViewController {
        screen.navigationController ?? screen
    }

    func fly(_ context: UIViewControllerContextTransitioning, toSlot: CGPoint) {
        let lifted = CGSize(width: -toSlot.x, height: -toSlot.y)

        screen.setCardHidden(true)
        if opening {
            screen.moveCard(by: toSlot)
            screen.setSurroundingsAlpha(0)
        } else {
            origin.moveOpenCard(by: lifted, clearingPocket: true, animated: false)
        }

        let animator = PocketCardFlightTiming.makeAnimator()
        animator.addAnimations { [self] in
            screen.moveCard(by: opening ? .zero : toSlot)
            screen.setBarAlpha(opening ? 1 : 0)
        }
        let pageFade = opening ? nil : UIViewPropertyAnimator(duration: Self.pageFadeOut, curve: .easeOut) { [self] in
            screen.setPageAlpha(0)
        }
        if opening {
            animator.addAnimations({ [self] in screen.setPageAlpha(1) }, delayFactor: Self.pageDelay)
        }
        animator.addCompletion { [self] _ in finish(context) }

        // The start is drawn before anything moves, so the Pocket animates from it.
        DispatchQueue.main.async { [self] in
            origin.moveOpenCard(by: opening ? lifted : .zero, clearingPocket: opening, animated: true)
            animator.startAnimation()
            pageFade?.startAnimation()
        }
    }

    /// The backdrop fades in for the Pocket to fade out behind the screen.
    func fade(_ context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        let backdrop = UIView(frame: container.bounds)
        backdrop.backgroundColor = .bgSurfaceMain
        container.insertSubview(backdrop, belowSubview: presented.view)

        let fading: [UIView] = [presented.view, backdrop]
        let alpha: CGFloat = opening ? 1 : 0
        if opening {
            screen.setSurroundingsAlpha(0)
            fading.forEach { $0.alpha = 0 }
        } else {
            origin.moveOpenCard(by: .zero, clearingPocket: false, animated: false)
        }

        let animator = PocketCardFlightTiming.makeAnimator()
        animator.addAnimations { [self] in
            screen.setSurroundingsAlpha(alpha)
            fading.forEach { $0.alpha = alpha }
        }
        animator.addCompletion { [self] _ in
            backdrop.removeFromSuperview()
            finish(context)
        }
        animator.startAnimation()
    }

    func finish(_ context: UIViewControllerContextTransitioning) {
        screen.endFlight()
        context.completeTransition(!context.transitionWasCancelled)
    }
}
