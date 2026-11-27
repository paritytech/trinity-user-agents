import UIKit
import UIKitExt

/// A screen that animates its own push and pop. Nil keeps the default transition.
///
/// On a `CustomTransitionNavigationController`, a swipe from the left edge scrubs the screen's pop, and the system
/// back swipes are off while the screen is on top.
protocol NavigationTransitionAnimating: AnyObject {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning?

    /// Whether a swipe from the left edge may start this screen's pop now. The swipe scrubs the pop when the
    /// screen's animator is a `BackSwipeScrubbing`.
    var allowsBackSwipe: Bool { get }
}

extension NavigationTransitionAnimating {
    var allowsBackSwipe: Bool {
        false
    }
}

/// A stack whose screens may draw their own push and pop (`NavigationTransitionAnimating`).
///
/// A swipe from the left edge scrubs such a screen's pop when its animator is a `BackSwipeScrubbing`. The system
/// back swipes are off while such a screen is on top: they would run UIKit's own pop, not the screen's animator.
final class CustomTransitionNavigationController: AppNavigationController {
    private lazy var backSwipe = UIScreenEdgePanGestureRecognizer(target: self, action: #selector(followBackSwipe))

    /// The pop the back swipe is scrubbing, from when it started until the finger lets go.
    private var scrubbedPop: BackSwipeScrubbing?

    override func viewDidLoad() {
        super.viewDidLoad()

        backSwipe.edges = .left
        backSwipe.delegate = self
        view.addGestureRecognizer(backSwipe)
    }

    override func navigationWillShow(_ viewController: UIViewController) {
        super.navigationWillShow(viewController)

        updateSystemBackSwipes()
    }

    @objc func navigationController(
        _: UINavigationController,
        animationControllerFor operation: UINavigationController.Operation,
        from fromViewController: UIViewController,
        to toViewController: UIViewController
    ) -> UIViewControllerAnimatedTransitioning? {
        let moving = operation == .push ? toViewController : fromViewController
        return (moving as? NavigationTransitionAnimating)?.navigationAnimator(for: operation)
    }

    @objc func navigationController(
        _: UINavigationController,
        interactionControllerFor animationController: UIViewControllerAnimatedTransitioning
    ) -> UIViewControllerInteractiveTransitioning? {
        guard isBackSwiping else { return nil }

        scrubbedPop = animationController as? BackSwipeScrubbing
        return scrubbedPop
    }
}

// MARK: - Private

private extension CustomTransitionNavigationController {
    var isBackSwiping: Bool {
        backSwipe.state == .began || backSwipe.state == .changed
    }

    @objc func followBackSwipe(_ gesture: UIScreenEdgePanGestureRecognizer) {
        let width = view.bounds.width
        let travel = gesture.translation(in: view).x
        let progress = BackSwipe.progress(travel: travel, width: width)
        switch gesture.state {
        case .began:
            popViewController(animated: true)
        case .changed:
            scrubbedPop?.update(progress: progress)
        case .ended:
            let velocity = gesture.velocity(in: view).x
            let commits = BackSwipe.commits(travel: travel, velocity: velocity, width: width)
            releaseBackSwipe(progress: progress, commits: commits)
        case .cancelled,
             .failed:
            releaseBackSwipe(progress: progress, commits: false)
        default:
            break
        }
    }

    func releaseBackSwipe(progress: CGFloat, commits: Bool) {
        scrubbedPop?.release(progress: progress, commits: commits)
        scrubbedPop = nil
    }

    /// Waits for the transition to end, since a cancelled pop leaves the screen it started from on top.
    func updateSystemBackSwipes() {
        guard let transitionCoordinator else {
            matchSystemBackSwipesToTopScreen()
            return
        }

        transitionCoordinator.animate(alongsideTransition: nil) { [weak self] _ in
            self?.matchSystemBackSwipesToTopScreen()
        }
    }

    func matchSystemBackSwipesToTopScreen() {
        setSystemBackSwipesEnabled(!(topViewController is NavigationTransitionAnimating))
    }
}

// MARK: - UIGestureRecognizerDelegate

/// The swipe begins on a screen that allows it, and other pans wait for it.
extension CustomTransitionNavigationController: UIGestureRecognizerDelegate {
    func gestureRecognizerShouldBegin(_: UIGestureRecognizer) -> Bool {
        guard viewControllers.count > 1, transitionCoordinator == nil else { return false }

        return (topViewController as? NavigationTransitionAnimating)?.allowsBackSwipe == true
    }

    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        other is UIPanGestureRecognizer
    }
}
