import UIKit

/// The swipe from the left edge that closes an opened card. The finger scrubs the card's close flight, and a
/// release finishes it or returns the card. Other pans wait for it, and it gives way at once to a touch that does
/// not start at the edge.
@MainActor
final class PocketEdgeSwipe: NSObject, UIGestureRecognizerDelegate {
    private weak var screen: PocketFlightScreen?
    private let canBegin: () -> Bool
    private lazy var pan = UIScreenEdgePanGestureRecognizer(target: self, action: #selector(followSwipe))
    private var flight: PocketFlight?

    init(screen: PocketFlightScreen, canBegin: @escaping () -> Bool) {
        self.screen = screen
        self.canBegin = canBegin
    }

    func install() {
        pan.edges = .left
        pan.delegate = self
        screen?.view.addGestureRecognizer(pan)
    }

    /// The flight to scrub, when the swipe started the pop `animator` animates.
    func interactor(for animator: UIViewControllerAnimatedTransitioning) -> UIViewControllerInteractiveTransitioning? {
        guard isSwiping, let flight = animator as? PocketFlight else { return nil }

        self.flight = flight
        return flight
    }

    func gestureRecognizerShouldBegin(_: UIGestureRecognizer) -> Bool {
        guard let screen, screen.transitionCoordinator == nil else { return false }

        return canBegin()
    }

    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        other is UIPanGestureRecognizer
    }
}

private extension PocketEdgeSwipe {
    var isSwiping: Bool {
        pan.state == .began || pan.state == .changed
    }

    @objc func followSwipe(_ gesture: UIScreenEdgePanGestureRecognizer) {
        guard let screen else { return }

        let width = screen.view.bounds.width
        let travel = gesture.translation(in: screen.view).x
        let progress = PocketFlightChoreography.progress(travel: travel, width: width)
        switch gesture.state {
        case .began:
            screen.navigationController?.popViewController(animated: true)
        case .changed:
            flight?.update(progress: progress)
        case .ended:
            let velocity = gesture.velocity(in: screen.view).x
            let commits = PocketFlightChoreography.commits(travel: travel, velocity: velocity, width: width)
            release(progress: progress, commits: commits)
        case .cancelled,
             .failed:
            release(progress: progress, commits: false)
        default:
            break
        }
    }

    func release(progress: CGFloat, commits: Bool) {
        flight?.release(progress: progress, commits: commits)
        flight = nil
    }
}
