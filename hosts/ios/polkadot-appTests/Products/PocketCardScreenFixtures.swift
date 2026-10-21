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

/// Where the Pocket shows the card, in window coordinates.
let pocketSlot = CGRect(x: 24, y: 420, width: 345, height: PocketCardSize.height)

/// How the Pocket draws its open card: which card it is, how far it is moved
/// from its slot, and whether the rest of the Pocket has cleared away for it.
struct PocketPlacement: Equatable {
    var openCard: PocketCardKey?
    var offset: CGSize = .zero
    var cleared = false
}

@MainActor
final class FakePocketCardOrigin: PocketCardOrigin {
    var slot: CGRect?
    private(set) var placement = PocketPlacement()
    /// Every placement the screen has asked for, in order.
    private(set) var moves: [PocketPlacement] = []
    /// Runs on every placement the screen asks for, as it asks.
    var onMove: ((PocketPlacement) -> Void)?

    init(slot: CGRect?) {
        self.slot = slot
    }

    func frame(of _: PocketCardKey) -> CGRect? {
        slot
    }

    var openCard: PocketCardKey? {
        placement.openCard
    }

    func setOpenCard(_ key: PocketCardKey?) {
        placement.openCard = key
    }

    func moveOpenCard(by offset: CGSize, clearingPocket: Bool, animated _: Bool) {
        placement.offset = offset
        placement.cleared = clearingPocket
        moves.append(placement)
        onMove?(placement)
    }
}

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

/// The Wallet tab's stack as the app builds it, in a window, with a plain
/// screen standing in for the Pocket at its root.
@MainActor
func walletStack() throws -> (navigation: AppNavigationController, window: UIWindow) {
    let navigation = AppNavigationController(rootViewController: UIViewController())
    return try (navigation, showing(navigation))
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

/// Dismisses what `presented` is showing over and returns once it has gone.
@MainActor
func dismiss(_ presented: UIViewController) async {
    await withCheckedContinuation { dismissed in
        presented.dismiss(animated: true) { dismissed.resume() }
    }
}

/// Pushes `screen` as the app opens a card and returns once it has landed.
@MainActor
func openCard(_ screen: PocketCardScreenViewController, on navigation: UINavigationController) async {
    navigation.pushViewController(screen, animated: true)
    await transitionFinished(in: navigation)
}

/// Opens `screen` on a Wallet stack of its own and returns the window it is
/// shown in, which the test keeps alive.
@MainActor
func openCard(_ screen: PocketCardScreenViewController) async throws -> UIWindow {
    let (navigation, window) = try walletStack()
    await openCard(screen, on: navigation)
    return window
}

/// Pops the card `screen` off its stack and returns once it has left it.
@MainActor
func closeCard(_ screen: UIViewController, animated: Bool = true) async {
    guard let navigation = screen.navigationController else { return }

    navigation.popViewController(animated: animated)
    // An unanimated pop finishes on the stack's next layout pass.
    navigation.view.layoutIfNeeded()
    await transitionFinished(in: navigation)
    _ = await eventually { screen.parent == nil }
}

/// Awaited rather than waited on with the run loop, which cannot deliver
/// UIKit's completion from inside the main actor's job.
@MainActor
func transitionFinished(in navigation: UINavigationController) async {
    guard let transition = navigation.transitionCoordinator else { return }

    await withCheckedContinuation { finished in
        transition.animate(alongsideTransition: nil) { _ in finished.resume() }
    }
}

/// Waits for `condition` for up to five seconds, letting the main actor run so
/// that UIKit can carry a transition on meanwhile.
@MainActor
func eventually(_ condition: () -> Bool) async -> Bool {
    for _ in 0 ..< 250 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return false
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
