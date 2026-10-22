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
