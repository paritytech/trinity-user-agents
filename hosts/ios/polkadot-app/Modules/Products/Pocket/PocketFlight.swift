import SwiftUI
import UIKit

/// The screen a flight carries: the item it draws, and how the item and the
/// screen's page move while the flight runs.
@MainActor
protocol PocketFlightScreen: UIViewController {
    var pocketItem: PocketItem { get }
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
    private static let pageFadeOut = PocketFlightTiming.duration * 0.2
    private static let crossfadeDuration: TimeInterval = 0.18

    enum Motion: Equatable {
        case fly(toSlot: CGPoint)
        case fade
        case crossfade
    }

    private let screen: PocketFlightScreen
    private let origin: PocketOrigin?
    private let opening: Bool
    /// Read once, so the duration UIKit is told and the motion that runs agree.
    private let reduceMotion: Bool

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

    func transitionDuration(using _: UIViewControllerContextTransitioning?) -> TimeInterval {
        reduceMotion ? Self.crossfadeDuration : PocketFlightTiming.duration
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
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

        let toSlot = origin?.frame(of: screen.pocketItem).map { slot in
            let item = screen.itemFrame(in: container)
            let landing = container.convert(slot, from: nil)
            return CGPoint(x: landing.minX - item.minX, y: landing.minY - item.minY)
        }

        switch Self.motion(toSlot: toSlot, reduceMotion: reduceMotion) {
        case let .fly(slot):
            fly(context, toSlot: slot)
        case .fade:
            fade(context, animator: PocketFlightTiming.makeAnimator())
        case .crossfade:
            fade(context, animator: UIViewPropertyAnimator(duration: Self.crossfadeDuration, curve: .easeInOut))
        }
    }
}

private extension PocketFlight {
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

    /// The backdrop fades in for the Pocket to fade out behind the screen.
    func fade(_ context: UIViewControllerContextTransitioning, animator: UIViewPropertyAnimator) {
        let container = context.containerView
        let backdrop = UIView(frame: container.bounds)
        backdrop.backgroundColor = .bgSurfaceMain
        container.insertSubview(backdrop, belowSubview: screen.view)

        let fading: [UIView] = [screen.view, backdrop]
        let alpha: CGFloat = opening ? 1 : 0
        if opening {
            screen.setPageAlpha(0)
            fading.forEach { $0.alpha = 0 }
        } else {
            origin?.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
        }

        animator.addAnimations { [self] in
            screen.setPageAlpha(alpha)
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
