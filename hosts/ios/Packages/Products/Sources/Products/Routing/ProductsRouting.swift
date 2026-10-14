import UIKit
import UIKitExt

/// Anchored presentation surface shared by every product flow. Flows add their
/// concrete `show…` methods as extensions that build a view and hand it to
/// `present(view:)`; when no view is anchored `present` returns `false` and the
/// flow delivers its own rejection so callers never hang.
@MainActor
public protocol ProductsRouting: AnyObject {
    func setPresentationView(_ view: ControllerBackedProtocol)
    var isReady: Bool { get }
    @discardableResult
    func present(view: ControllerBackedProtocol) -> Bool
}

/// The one shared product router: holds the presentation anchor and presents
/// on its topmost controller. Per-flow presentation lives in `ProductsRouting`
/// extensions co-located with each flow's view factory.
@MainActor
public final class ProductsRouter: ProductsRouting {
    public private(set) weak var presentationView: ControllerBackedProtocol?

    public nonisolated init() {}

    public func setPresentationView(_ view: ControllerBackedProtocol) {
        presentationView = view
    }

    public var isReady: Bool {
        presentationView != nil
    }

    @discardableResult
    public func present(view: ControllerBackedProtocol) -> Bool {
        guard let presentationView else {
            return false
        }

        presentationView.controller.topmostPresented.present(view.controller, animated: true)
        return true
    }
}
