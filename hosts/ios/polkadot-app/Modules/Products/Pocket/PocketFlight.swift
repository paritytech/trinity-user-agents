import SwiftUI
import UIKit

/// The screen a flight carries: the item it draws, and how the item and the
/// screen's page move while the flight runs.
@MainActor
protocol PocketFlightScreen: PocketItemScreen {
    func itemFrame(in view: UIView) -> CGRect
    func beginFlight()
    func moveItem(by offset: CGPoint)
    func setItemHidden(_ hidden: Bool)
    func setPageAlpha(_ alpha: CGFloat)
    func endFlight()
}

/// One clock for everything a flight moves, in UIKit and in SwiftUI alike, so
/// the item, its screen and the Pocket stay in step.
enum PocketFlightTiming {
    static let duration: TimeInterval = 0.5

    /// Android's curve, which carries the item slightly past its place.
    private static let curveStart = CGPoint(x: 0.34, y: 1.2)
    private static let curveEnd = CGPoint(x: 0.64, y: 1)

    static func animation(duration: TimeInterval) -> Animation {
        .timingCurve(curveStart.x, curveStart.y, curveEnd.x, curveEnd.y, duration: duration)
    }

    static func makeAnimator(duration: TimeInterval = PocketFlightTiming.duration) -> UIViewPropertyAnimator {
        UIViewPropertyAnimator(
            duration: duration,
            timingParameters: UICubicTimingParameters(controlPoint1: curveStart, controlPoint2: curveEnd)
        )
    }
}

/// Carries an opened item between its place in the Pocket and the top of its
/// screen. The Pocket draws the item while it moves, so the item keeps its depth
/// in the stack; the screen carries its page under the item, and draws the item
/// itself only at rest. An item the Pocket cannot place when the flight starts
/// fades with the rest of the screen: the Pocket may no longer be what the
/// screen opens from or closes to.
final class PocketFlight: NSObject, UIViewControllerAnimatedTransitioning {
    /// The page is shown only once the Pocket's cards have left it, and is gone
    /// before they come back over it.
    private static let pageDelay: CGFloat = 0.35
    private static let pageFadeOut = PocketFlightTiming.duration * Double(PocketFlightChoreography.pageFadeOutFraction)
    static let crossfadeDuration: TimeInterval = 0.18

    enum Motion: Equatable {
        case fly(toSlot: CGPoint)
        case fade
        case crossfade
    }

    private struct Scrub {
        let context: UIViewControllerContextTransitioning
        let choreography: PocketFlightChoreography
        /// Covers the Pocket under a screen that fades, and fades with it.
        let backdrop: UIView?
    }

    private struct Release {
        let progress: CGFloat
        let commits: Bool
    }

    private let screen: PocketFlightScreen
    private let origin: PocketOrigin?
    private let opening: Bool
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool
    /// The close a finger is scrubbing, from when UIKit starts it until the finger lets go.
    private var scrub: Scrub?
    /// A release that came before UIKit started the transition.
    private var earlyRelease: Release?

    init(screen: PocketFlightScreen, origin: PocketOrigin?, opening: Bool) {
        self.screen = screen
        self.origin = origin
        self.opening = opening
        reduceMotion = UIAccessibility.isReduceMotionEnabled
    }

    /// Reduce Motion crossfades in place of any slide, and an item with no slot
    /// fades, since there is nowhere to fly to.
    static func motion(toSlot: CGPoint?, reduceMotion: Bool) -> Motion {
        if reduceMotion { return .crossfade }
        guard let toSlot else { return .fade }

        return .fly(toSlot: toSlot)
    }

    /// Fades `screen` in or out over the Pocket, for an item that cannot fly. The caller adds its own animations to
    /// `animator`, and completes the transition in `completion`.
    static func fade(
        _ context: UIViewControllerContextTransitioning,
        screen: UIViewController,
        origin: PocketOrigin?,
        opening: Bool,
        animator: UIViewPropertyAnimator,
        then completion: @escaping () -> Void
    ) {
        let container = context.containerView
        let backdrop = insertBackdrop(below: screen.view, in: container)

        let fading: [UIView] = [screen.view, backdrop]
        let alpha: CGFloat = opening ? 1 : 0
        if opening {
            fading.forEach { $0.alpha = 0 }
        } else {
            origin?.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
        }

        animator.addAnimations {
            fading.forEach { $0.alpha = alpha }
        }
        animator.addCompletion { _ in
            backdrop.removeFromSuperview()
            completion()
        }
        animator.startAnimation()
    }

    /// The animator a fade runs on: the flight's clock, or a short crossfade under Reduce Motion.
    static func fallbackAnimator(reduceMotion: Bool) -> UIViewPropertyAnimator {
        reduceMotion
            ? UIViewPropertyAnimator(duration: crossfadeDuration, curve: .easeInOut)
            : PocketFlightTiming.makeAnimator()
    }

    static func duration(reduceMotion: Bool) -> TimeInterval {
        reduceMotion ? crossfadeDuration : PocketFlightTiming.duration
    }

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        Self.duration(reduceMotion: reduceMotion)
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        guard !context.isInteractive else { return }

        switch Self.motion(toSlot: prepare(context), reduceMotion: reduceMotion) {
        case let .fly(slot):
            fly(context, toSlot: slot)
        case .fade,
             .crossfade:
            fadeInPlace(context)
        }
    }
}

extension PocketFlight: UIViewControllerInteractiveTransitioning {
    /// A close that fades is scrubbed too: the screen and its backdrop fade under the finger.
    func startInteractiveTransition(_ context: UIViewControllerContextTransitioning) {
        let toSlot = prepare(context)
        if case .fly = Self.motion(toSlot: toSlot, reduceMotion: reduceMotion) {
            screen.setItemHidden(true)
            scrub = Scrub(context: context, choreography: PocketFlightChoreography(toSlot: toSlot), backdrop: nil)
        } else {
            let backdrop = Self.insertBackdrop(below: screen.view, in: context.containerView)
            scrub = Scrub(context: context, choreography: PocketFlightChoreography(toSlot: nil), backdrop: backdrop)
        }
        update(progress: 0)

        if let earlyRelease {
            release(progress: earlyRelease.progress, commits: earlyRelease.commits)
        }
    }

    /// Draws the close at `progress`, from 0, the item open, to 1, the item back in its slot.
    func update(progress: CGFloat) {
        guard let scrub else { return }

        apply(scrub.choreography.frame(progress: progress), backdrop: scrub.backdrop)
        scrub.context.updateInteractiveTransition(progress)
    }

    /// Runs the rest of the close, or the way back to the open screen, from where the finger left it.
    func release(progress: CGFloat, commits: Bool) {
        guard let scrub else {
            earlyRelease = Release(progress: progress, commits: commits)
            return
        }

        self.scrub = nil
        let context = scrub.context
        let backdrop = scrub.backdrop
        let end = scrub.choreography.frame(progress: commits ? 1 : 0)
        let duration = scrub.choreography.releaseDuration(remaining: commits ? 1 - progress : progress)

        let animator = PocketFlightTiming.makeAnimator(duration: duration)
        animator.addAnimations { [self] in
            screen.moveItem(by: end.itemOffset)
            screen.setPageAlpha(end.pageAlpha)
            screen.view.alpha = end.screenAlpha
            backdrop?.alpha = end.screenAlpha
        }
        animator.addCompletion { [self] _ in
            backdrop?.removeFromSuperview()
            finish(context)
        }

        if commits {
            context.finishInteractiveTransition()
        } else {
            context.cancelInteractiveTransition()
        }
        origin?.moveOpenItem(by: end.pocketOffset, clearance: end.clearance, animatedOver: duration)
        animator.startAnimation()
    }
}

private extension PocketFlight {
    static func insertBackdrop(below view: UIView, in container: UIView) -> UIView {
        let backdrop = UIView(frame: container.bounds)
        backdrop.backgroundColor = .bgSurfaceMain
        container.insertSubview(backdrop, belowSubview: view)
        return backdrop
    }

    /// Places both screens for the flight and measures the way from the item's place to its slot, which is nil
    /// when the Pocket cannot place the item.
    func prepare(_ context: UIViewControllerContextTransitioning) -> CGPoint? {
        let container = context.containerView
        if opening {
            screen.view.frame = context.finalFrame(for: screen)
            container.addSubview(screen.view)
        } else if let pocket = context.viewController(forKey: .to) {
            pocket.view.frame = context.finalFrame(for: pocket)
            container.insertSubview(pocket.view, belowSubview: screen.view)
        }

        screen.beginFlight()
        container.layoutIfNeeded()

        return origin?.frame(of: screen.pocketItem).map { slot in
            let item = screen.itemFrame(in: container)
            let landing = container.convert(slot, from: nil)
            return CGPoint(x: landing.minX - item.minX, y: landing.minY - item.minY)
        }
    }

    func apply(_ frame: PocketFlightFrame, backdrop: UIView?) {
        screen.moveItem(by: frame.itemOffset)
        screen.setPageAlpha(frame.pageAlpha)
        screen.view.alpha = frame.screenAlpha
        backdrop?.alpha = frame.screenAlpha
        origin?.moveOpenItem(by: frame.pocketOffset, clearance: frame.clearance, animatedOver: nil)
    }

    func fly(_ context: UIViewControllerContextTransitioning, toSlot: CGPoint) {
        let lifted = CGSize(width: -toSlot.x, height: -toSlot.y)

        screen.setItemHidden(true)
        if opening {
            screen.moveItem(by: toSlot)
            screen.setPageAlpha(0)
        } else {
            origin?.moveOpenItem(by: lifted, clearingPocket: true, animated: false)
        }

        let animator = PocketFlightTiming.makeAnimator()
        animator.addAnimations { [self] in
            screen.moveItem(by: opening ? .zero : toSlot)
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
            origin?.moveOpenItem(by: opening ? lifted : .zero, clearingPocket: opening, animated: true)
            animator.startAnimation()
            pageFade?.startAnimation()
        }
    }

    /// The screen's page fades with the screen, on the same animator the shared fade runs.
    func fadeInPlace(_ context: UIViewControllerContextTransitioning) {
        let alpha: CGFloat = opening ? 1 : 0
        if opening {
            screen.setPageAlpha(0)
        }

        let animator = Self.fallbackAnimator(reduceMotion: reduceMotion)
        animator.addAnimations { [self] in
            screen.setPageAlpha(alpha)
        }
        Self.fade(context, screen: screen, origin: origin, opening: opening, animator: animator) { [self] in
            finish(context)
        }
    }

    func finish(_ context: UIViewControllerContextTransitioning) {
        screen.endFlight()
        context.completeTransition(!context.transitionWasCancelled)
    }
}
