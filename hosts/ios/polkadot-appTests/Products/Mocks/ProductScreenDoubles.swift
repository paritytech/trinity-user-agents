import Products
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

/// A product's page, as the visibility rules see it.
final class StubProductPage: UIViewController, ProductPageShowing, ControllerBackedProtocol {
    let productId: ProductId

    init(productId: ProductId) {
        self.productId = productId
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

@MainActor
final class StubApplication: ApplicationStateProviding {
    var applicationState: UIApplication.State = .active
}

/// A visible window in the test host's scene, so UIKit really presents and
/// dismisses over `root`.
@MainActor
func makeTestWindow(root: UIViewController) throws -> UIWindow {
    let scene = try #require(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
    let window = UIWindow(windowScene: scene)
    window.rootViewController = root
    window.isHidden = false
    return window
}

@MainActor
func present(_ controller: UIViewController, over presenter: UIViewController) async {
    await withCheckedContinuation { continuation in
        presenter.present(controller, animated: false) { continuation.resume() }
    }
}

/// UIKit finishes presenting and dismissing on later run loop turns.
@MainActor
func waitUntil(_ condition: () -> Bool) async throws {
    for _ in 0 ..< 300 where !condition() {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(condition())
}
