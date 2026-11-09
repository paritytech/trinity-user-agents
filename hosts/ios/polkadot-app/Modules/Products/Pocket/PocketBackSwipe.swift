import UIKit

/// The back swipe every Pocket item screen shares: a swipe from the left edge that closes the screen once the
/// finger lifts, so the item's full close plays rather than the screen following the finger. The stack's own back
/// swipes are off while the screen shows, since they would pop beside it.
@MainActor
final class PocketBackSwipe: NSObject {
    private weak var screen: UIViewController?
    private let edgePan = UIScreenEdgePanGestureRecognizer()

    init(screen: UIViewController) {
        self.screen = screen
        super.init()
    }

    /// Adds the edge pan to the screen's view. Each pan in `pans` waits for it, so a swipe from the edge never
    /// reaches the page.
    func install(ahead pans: [UIGestureRecognizer]) {
        edgePan.edges = .left
        edgePan.addTarget(self, action: #selector(swiped))
        screen?.view.addGestureRecognizer(edgePan)
        pans.forEach { $0.require(toFail: edgePan) }
    }

    func setSystemBackSwipesEnabled(_ enabled: Bool) {
        screen?.navigationController?.interactivePopGestureRecognizer?.isEnabled = enabled
        if #available(iOS 26, *) {
            screen?.navigationController?.interactiveContentPopGestureRecognizer?.isEnabled = enabled
        }
    }
}

private extension PocketBackSwipe {
    @objc func swiped() {
        guard edgePan.state == .ended, edgePan.velocity(in: edgePan.view).x >= 0 else { return }

        screen?.navigationController?.popViewController(animated: true)
    }
}
