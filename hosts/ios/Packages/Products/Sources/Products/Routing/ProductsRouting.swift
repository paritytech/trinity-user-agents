import UIKit
import UIKitExt

/// Anchored presentation surface shared by every product flow. Flows add their
/// concrete `show…` methods as extensions that build a view and hand it to
/// `present(view:)`; when no view is anchored `present` returns `false` and the
/// flow delivers its own rejection so callers never hang. A view that cannot
/// be put on screen after all, such as one asked for while the topmost screen
/// is closing, is reported through `onUndelivered` for the same reason.
@MainActor
public protocol ProductsRouting: AnyObject {
    func setPresentationView(_ view: ControllerBackedProtocol)
    var isReady: Bool { get }
    @discardableResult
    func present(view: ControllerBackedProtocol, onUndelivered: (@MainActor () -> Void)?) -> Bool
}

public extension ProductsRouting {
    @discardableResult
    func present(view: ControllerBackedProtocol) -> Bool {
        present(view: view, onUndelivered: nil)
    }
}

/// The one shared product router: holds the presentation anchor and presents
/// on its topmost controller. Per-flow presentation lives in `ProductsRouting`
/// extensions co-located with each flow's view factory.
@MainActor
public final class ProductsRouter: ProductsRouting {
    public private(set) weak var presentationView: ControllerBackedProtocol?

    /// Where to present while nothing is anchored, for a flow with no screen
    /// of its own, such as a worker running headless.
    private let fallback: (@MainActor @Sendable () -> ControllerBackedProtocol?)?

    public nonisolated init(fallback: (@MainActor @Sendable () -> ControllerBackedProtocol?)? = nil) {
        self.fallback = fallback
    }

    public func setPresentationView(_ view: ControllerBackedProtocol) {
        presentationView = view
    }

    public var isReady: Bool {
        anchor != nil
    }

    @discardableResult
    public func present(view: ControllerBackedProtocol, onUndelivered: (@MainActor () -> Void)?) -> Bool {
        guard anchor != nil else {
            return false
        }

        deliver(view, attemptsLeft: Self.presentAttempts, onUndelivered: onUndelivered)
        return true
    }

    /// How many times a presentation is tried while the topmost screen is
    /// still opening or closing, `presentRetryDelay` apart: about two seconds.
    private static let presentAttempts = 8
    private static let presentRetryDelay: TimeInterval = 0.25

    /// Present `view` on the topmost screen once it is settled. UIKit drops a
    /// presentation on a screen that is closing or off the window without
    /// saying so, which would leave the flow waiting on a prompt nobody sees.
    private func deliver(
        _ view: ControllerBackedProtocol,
        attemptsLeft: Int,
        onUndelivered: (@MainActor () -> Void)?
    ) {
        guard let host = anchor?.controller.topmostPresented else {
            onUndelivered?()
            return
        }

        let settled = host.viewIfLoaded?.window != nil && !host.isBeingDismissed &&
            !host.isBeingPresented && host.transitionCoordinator == nil
        if settled {
            host.present(view.controller, animated: true)
            if view.controller.presentingViewController != nil {
                return
            }
        }

        guard attemptsLeft > 1 else {
            onUndelivered?()
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.presentRetryDelay) { [weak self] in
            guard let self else {
                onUndelivered?()
                return
            }
            deliver(view, attemptsLeft: attemptsLeft - 1, onUndelivered: onUndelivered)
        }
    }

    private var anchor: ControllerBackedProtocol? {
        presentationView ?? fallback?()
    }
}
