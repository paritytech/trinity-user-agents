import DesignSystem
import UIKit

/// Carries the Collectibles book from the Pocket's peek to the top of its screen. The book lifts out of the peek and
/// settles on the page. A screen with no peek to fly from fades in instead.
final class CollectiblesBookFlight: NSObject, UIViewControllerAnimatedTransitioning {
    private let screen: CollectiblesViewController
    private let origin: PocketOrigin?
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool

    init(screen: CollectiblesViewController, origin: PocketOrigin?) {
        self.screen = screen
        self.origin = origin
        reduceMotion = UIAccessibility.isReduceMotionEnabled
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        PocketFlight.duration(reduceMotion: reduceMotion)
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        screen.view.frame = context.finalFrame(for: screen)
        container.addSubview(screen.view)
        screen.view.layoutIfNeeded()

        switch PocketFlight.motion(
            toSlot: slotOffset(of: screen, origin: origin, in: container),
            reduceMotion: reduceMotion
        ) {
        case let .fly(offset):
            screen.beginFlight(from: offset)

            let backdrop = backdropFade(of: screen, to: .bgSurfaceMain)
            let flight = flightAnimator(moving: screen, to: .zero)
            flight.addCompletion { _ in
                context.completeTransition(!context.transitionWasCancelled)
            }

            origin?.moveOpenItem(by: .zero, clearingPocket: true, animated: true)
            backdrop.startAnimation()
            flight.startAnimation()
        case .fade,
             .crossfade:
            PocketFlight.fade(
                context,
                screen: screen,
                origin: origin,
                opening: true,
                animator: PocketFlight.fallbackAnimator(reduceMotion: reduceMotion)
            ) {
                context.completeTransition(!context.transitionWasCancelled)
            }
        }
    }
}

/// Carries the Collectibles book from the top of its screen back to the Pocket's peek, then closes the screen.
/// A screen with no peek to fly to fades out instead.
final class CollectiblesBookReturn: NSObject, UIViewControllerAnimatedTransitioning {
    private enum Scrub {
        /// The page shrinks or fades with the finger, and the book flies back by `offset` once a release commits.
        case page(UIViewControllerContextTransitioning, offset: CGPoint)
        /// The whole screen fades over the Pocket, with the backdrop that covers the Pocket under it.
        case fade(UIViewControllerContextTransitioning, PocketFadeScrub)

        var context: UIViewControllerContextTransitioning {
            switch self {
            case let .page(context, _),
                 let .fade(context, _):
                context
            }
        }
    }

    private struct Release {
        let progress: CGFloat
        let commits: Bool
    }

    private let screen: CollectiblesViewController
    private let origin: PocketOrigin?
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool
    /// The close a finger is scrubbing, from when UIKit starts it until the finger lets go.
    private var scrub: Scrub?
    /// A release that came before UIKit started the transition.
    private var earlyRelease: Release?

    init(screen: CollectiblesViewController, origin: PocketOrigin?) {
        self.screen = screen
        self.origin = origin
        reduceMotion = UIAccessibility.isReduceMotionEnabled
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        PocketFlight.duration(reduceMotion: reduceMotion)
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        guard !context.isInteractive else { return }

        let container = context.containerView
        placeWallet(context)

        switch PocketFlight.motion(
            toSlot: slotOffset(of: screen, origin: origin, in: container),
            reduceMotion: reduceMotion
        ) {
        case let .fly(offset):
            screen.close { [self] in
                flyBack(to: offset, context)
            }
        case .fade,
             .crossfade:
            PocketFlight.fade(
                context,
                screen: screen,
                origin: origin,
                opening: false,
                animator: PocketFlight.fallbackAnimator(reduceMotion: reduceMotion)
            ) {
                context.completeTransition(!context.transitionWasCancelled)
            }
        }
    }
}

extension CollectiblesBookReturn: BackSwipeScrubbing {
    /// A close that fades is scrubbed too: the screen and its backdrop fade under the finger.
    func startInteractiveTransition(_ context: UIViewControllerContextTransitioning) {
        placeWallet(context)
        let container = context.containerView

        switch PocketFlight.motion(
            toSlot: slotOffset(of: screen, origin: origin, in: container),
            reduceMotion: reduceMotion
        ) {
        case let .fly(offset):
            screen.beginScrub()
            scrub = .page(context, offset: offset)
        case .fade,
             .crossfade:
            scrub = .fade(context, PocketFadeScrub(context, screen: screen, origin: origin))
        }
        update(progress: 0)

        if let earlyRelease {
            release(progress: earlyRelease.progress, commits: earlyRelease.commits)
        }
    }

    /// Draws the close at `progress`, from 0, the screen open, to 1, the screen closed.
    func update(progress: CGFloat) {
        guard let scrub else { return }

        switch scrub {
        case .page:
            screen.scrub(to: progress)
        case let .fade(_, fade):
            fade.update(progress: progress)
        }
        scrub.context.updateInteractiveTransition(progress)
    }

    /// Runs the rest of the close, or the way back to the open screen, from where the finger left it.
    func release(progress: CGFloat, commits: Bool) {
        guard let scrub else {
            earlyRelease = Release(progress: progress, commits: commits)
            return
        }

        self.scrub = nil
        if commits {
            scrub.context.finishInteractiveTransition()
        } else {
            scrub.context.cancelInteractiveTransition()
        }

        switch scrub {
        case let .page(context, offset):
            releasePage(context, offset: offset, commits: commits)
        case let .fade(context, fade):
            fade.release(progress: progress, commits: commits) {
                context.completeTransition(commits)
            }
        }
    }
}

private extension CollectiblesBookReturn {
    func placeWallet(_ context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        if let wallet = context.viewController(forKey: .to) {
            container.insertSubview(wallet.view, belowSubview: screen.view)
            wallet.view.frame = context.finalFrame(for: wallet)
        }
    }

    func flyBack(to offset: CGPoint, _ context: UIViewControllerContextTransitioning) {
        let backdrop = backdropFade(of: screen, to: .clear)
        let flight = flightAnimator(moving: screen, to: offset)
        flight.addCompletion { _ in
            context.completeTransition(!context.transitionWasCancelled)
        }

        origin?.moveOpenItem(by: .zero, clearingPocket: false, animated: true)
        backdrop.startAnimation()
        flight.startAnimation()
    }

    func releasePage(_ context: UIViewControllerContextTransitioning, offset: CGPoint, commits: Bool) {
        guard commits else {
            screen.endScrub(finishing: false) {
                context.completeTransition(false)
            }
            return
        }

        screen.endScrub(finishing: true) { [self] in
            screen.close { [self] in
                flyBack(to: offset, context)
            }
        }
    }
}

/// The offset that moves a book resting in `container` onto the Pocket's peek, or nil when the Pocket has no peek.
/// The book's own frame is unchanged by the close that comes before a return, since the lift moves a subview.
@MainActor
private func slotOffset(
    of screen: CollectiblesViewController,
    origin: PocketOrigin?,
    in container: UIView
) -> CGPoint? {
    origin?.frame(of: .collectibles).map { slot in
        offsetToSlot(container.convert(slot, from: nil), fromLanding: screen.bookFrame(in: container))
    }
}

/// The offset that moves a book resting at `landing` onto `slot`, both in the same coordinates.
private func offsetToSlot(_ slot: CGRect, fromLanding landing: CGRect) -> CGPoint {
    CGPoint(x: slot.minX - landing.minX, y: slot.minY - landing.minY)
}

/// The book's flight to `offset` from its place, on the spring the push and the return share.
private func flightAnimator(moving screen: CollectiblesViewController, to offset: CGPoint) -> UIViewPropertyAnimator {
    let spring = UISpringTimingParameters(
        mass: 1,
        stiffness: CollectiblesBookTiming.flightStiffness,
        damping: 2 * sqrt(CollectiblesBookTiming.flightStiffness),
        initialVelocity: .zero
    )
    let flight = UIViewPropertyAnimator(duration: 0, timingParameters: spring)
    flight.addAnimations {
        screen.moveBook(by: offset)
    }
    return flight
}

/// Fades the screen's backdrop to `color`, so the Wallet shows through or is covered again.
private func backdropFade(of screen: CollectiblesViewController, to color: UIColor) -> UIViewPropertyAnimator {
    UIViewPropertyAnimator(duration: CollectiblesBookTiming.backdropFade, curve: .linear) {
        screen.view.backgroundColor = color
    }
}
