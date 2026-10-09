import Products
import Testing
import UIKit
@testable import polkadot_app

let loyaltyCard = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

/// Puts `root` in a window of the app's scene. Only there does a screen it
/// presents join the window, as it does in the app.
@MainActor
func showing(_ root: UIViewController) throws -> UIWindow {
    let scene = try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene)
    let window = UIWindow(windowScene: scene)
    window.rootViewController = root
    window.makeKeyAndVisible()
    window.layoutIfNeeded()
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

/// Presents `screen` as the app presents a card and returns the window it is
/// shown in, which the test keeps alive.
@MainActor
func presentCard(_ screen: UIViewController) async throws -> UIWindow {
    let presenter = UIViewController()
    let window = try showing(presenter)
    await present(cardNavigation(screen), from: presenter)
    return window
}

/// Closes the card `screen` is presented in and returns once it has closed.
@MainActor
func closeCard(_ screen: UIViewController) async {
    await withCheckedContinuation { closed in
        screen.dismiss(animated: true) { closed.resume() }
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
