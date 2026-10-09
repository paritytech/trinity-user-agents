import Products
import Testing
import UIKit
@testable import polkadot_app

// The card and the screen size the tests of an opened card share, so a face
// sized against one is never checked against another.

let loyaltyCard = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

let screenSize = CGSize(width: 393, height: 800)

/// Puts `screen` in a window, which a card's screen needs before its face may move.
@MainActor
func showing(_ screen: UIViewController) -> UIWindow {
    let window = UIWindow(frame: CGRect(origin: .zero, size: screenSize))
    window.rootViewController = screen
    window.isHidden = false
    window.layoutIfNeeded()
    return window
}

/// Puts `root` in a window of the app's scene. Only there does a screen it
/// presents join the window, as it does in the app.
@MainActor
func showingInScene(_ root: UIViewController) throws -> UIWindow {
    let scene = try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene)
    let window = UIWindow(windowScene: scene)
    window.rootViewController = root
    window.makeKeyAndVisible()
    return window
}

/// Wraps a card's screen as the app presents it: the root of a full-screen
/// navigation controller.
@MainActor
func cardNavigation(_ screen: UIViewController) -> UINavigationController {
    let navigation = AppNavigationController(rootViewController: screen)
    navigation.modalPresentationStyle = .fullScreen
    return navigation
}

/// Presents and returns once the presentation has finished. Awaited rather
/// than waited on with the run loop, which cannot deliver UIKit's completion
/// from inside the main actor's job.
@MainActor
func present(_ presented: UIViewController, from presenter: UIViewController) async {
    await withCheckedContinuation { finished in
        presenter.present(presented, animated: true) { finished.resume() }
    }
}

/// Dismisses what `presenter` presents and returns once that has finished.
@MainActor
func dismissPresented(from presenter: UIViewController) async {
    await withCheckedContinuation { finished in
        presenter.dismiss(animated: true) { finished.resume() }
    }
}

/// Turns the run loop until `condition` holds, for up to five seconds.
@MainActor
func waitUntil(on screen: UIViewController, _ condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(5)

    while Date() < deadline {
        screen.view.layoutIfNeeded()

        if condition() { return true }

        RunLoop.main.run(until: Date().addingTimeInterval(0.02))
    }

    return false
}

extension PocketCardScreenViewController {
    var scrollView: UIScrollView? {
        view.subviews.compactMap { $0 as? UIScrollView }.first
    }
}
