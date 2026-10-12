import Products
import TrUAPIHost
import UIKit
import UIKitExt

/// A screen that shows one product's page.
@MainActor
protocol ProductPageShowing: UIViewController {
    var productId: ProductId { get }
}

extension SPAViewController: ProductPageShowing {
    var productId: ProductId { configuration.page.host.toDotDomain() }
}

extension SPASimplifiedViewController: ProductPageShowing {
    var productId: ProductId { configuration.page.host.toDotDomain() }
}

/// Whether the user can see the product asking, so a host screen may open
/// for it. An App needs its page to be the top screen, whether a tab or a
/// modal. A Worker has no page, so the app in front is enough.
struct ProductVisibility {
    private let application: ApplicationStateProviding
    private let window: @MainActor () -> UIWindow?

    init(
        application: ApplicationStateProviding,
        window: @escaping @MainActor () -> UIWindow? = { UIWindow.keyWindow }
    ) {
        self.application = application
        self.window = window
    }

    @MainActor
    func canShow(productId: ProductId, executionKind: ProductExecutionKind) -> Bool {
        guard application.applicationState == .active,
              let top = window()?.topmostViewController,
              !isMoving(top) else {
            return false
        }
        switch executionKind {
        case .worker:
            return true
        case .app,
             .widget:
            return top.showsPage(of: productId)
        }
    }

    /// Gives a host screen over the page, such as the last scanner, time to
    /// finish opening and then closing. Nothing else is waited for.
    @MainActor
    func waitUntilVisible(productId: ProductId, executionKind: ProductExecutionKind) async -> Bool {
        for _ in 0 ..< 40 {
            if canShow(productId: productId, executionKind: executionKind) {
                return true
            }
            guard application.applicationState == .active,
                  let top = window()?.topmostViewController,
                  isMoving(top) else {
                return false
            }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return canShow(productId: productId, executionKind: executionKind)
    }

    /// A screen still opening or closing cannot be presented over.
    @MainActor
    private func isMoving(_ top: UIViewController) -> Bool {
        sequence(first: top, next: \.parent).contains { $0.isBeingPresented || $0.isBeingDismissed }
    }
}

private extension UIViewController {
    /// An open Pocket card shows the product's page as a child of its screen.
    func showsPage(of productId: ProductId) -> Bool {
        (self as? ProductPageShowing)?.productId == productId || children.contains { $0.showsPage(of: productId) }
    }
}
