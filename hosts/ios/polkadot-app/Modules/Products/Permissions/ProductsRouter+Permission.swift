import ObjectiveC
import Products
import UIKit
import UIKitExt

/// Presents ``ProductPermissionPrompt`` as a bottom sheet above the host product view controller.
///
/// A prompt that no screen shows, or that goes off the screen before the user answers, ends the request with
/// ``ProductPermissionPromptError``. That is not a refusal: the core keeps no decision, and the next request asks
/// again. A denial is kept only when the user selects "Deny".
extension ProductsRouter: @retroactive ProductPermissionRouting {
    public func showPrompt(context: ProductPermissionContext) {
        let promptView = ProductPermissionPromptViewFactory.createView(context: context)
        ProductPermissionPromptLifetime.attach(to: promptView.controller, context: context)

        // `present(view:)` gives true when a view is attached, but UIKit does not present from a view that is not in
        // a window. Then the prompt has no presenting controller.
        guard present(view: promptView), promptView.controller.presentingViewController != nil else {
            context.abandon(.cannotShow)
            return
        }

        #if targetEnvironment(simulator)
            TrUAPIE2EPendingPermission.show(context, controller: promptView.controller)
        #endif
    }
}

/// Ends the wait of a prompt when its controller goes away with no answer, for example when the room screen closes
/// and takes the sheet with it. After an answer, the abandon does nothing.
private final class ProductPermissionPromptLifetime: NSObject {
    private static var key: UInt8 = 0

    private let context: ProductPermissionContext

    private init(context: ProductPermissionContext) {
        self.context = context
    }

    @MainActor
    static func attach(to controller: UIViewController, context: ProductPermissionContext) {
        objc_setAssociatedObject(
            controller,
            &key,
            ProductPermissionPromptLifetime(context: context),
            .OBJC_ASSOCIATION_RETAIN_NONATOMIC
        )
    }

    deinit {
        Task { @MainActor [context] in context.abandon(.closedWithoutAnswer) }
    }
}

#if targetEnvironment(simulator)
    /// The permission prompt that waits for an answer, for the truapi E2E harness. The harness cannot tap the
    /// simulator, so it answers with the host action `permission` (`always`, `once` or `deny`).
    @MainActor
    enum TrUAPIE2EPendingPermission {
        private weak static var context: ProductPermissionContext?
        private weak static var controller: UIViewController?

        static func show(_ context: ProductPermissionContext, controller: UIViewController) {
            guard TrUAPIE2EMarkers.isEnabled else { return }
            self.context = context
            self.controller = controller
            let names = context.permissions.map { String(describing: $0) }.joined(separator: ",")
            Logger.shared.debug("[e2e:ui] prompt \(context.productId) \(names)")
        }

        static func answer(_ argument: String?) {
            let decision: PermissionDecision? =
                switch argument {
                case "always": .allowAlways
                case "once": .allowOnce
                case "deny": .deny
                default: nil
                }
            guard let context, let decision else {
                Logger.shared.warning("[e2e:ui] no prompt for the answer \(argument ?? "")")
                return
            }
            context.deliver(decision)
            controller?.dismiss(animated: false)
            Logger.shared.debug("[e2e:ui] prompt-answer \(context.productId) \(argument ?? "")")
            self.context = nil
            controller = nil
        }
    }
#endif
