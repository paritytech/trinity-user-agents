import UIKit

/// A presentation that overlays the current screen without replacing it (a tip, a transient
/// hint). `topmostPresented` looks past it, so navigation and re-auth still resolve to the
/// real screen underneath.
@MainActor
public protocol TransientPresentationSkipping: AnyObject {}

public extension UIViewController {
    /// The controller that should receive a `present` call when `self` is used
    /// as a presentation anchor. Resolved at call time so sheets land above any
    /// modal already covering the anchor instead of silently failing to present.
    var topmostPresented: UIViewController {
        if let navigationController = self as? UINavigationController,
           let visibleViewController = navigationController.visibleViewController {
            return visibleViewController.topmostPresented
        }

        if let presentedViewController,
           !(presentedViewController is TransientPresentationSkipping) {
            return presentedViewController.topmostPresented
        }

        if let provider = self as? TopmostChildProviding,
           let topmostChild = provider.topmostChild {
            return topmostChild.topmostPresented
        }

        return self
    }
}
