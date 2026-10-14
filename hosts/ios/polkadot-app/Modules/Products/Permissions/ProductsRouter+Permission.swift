import Products
import UIKitExt

/// Presents ``ProductPermissionPrompt`` as a bottom sheet above the host
/// product view controller; delivers a denial when no view is attached.
extension ProductsRouter: @retroactive ProductPermissionRouting {
    public func showPrompt(context: ProductPermissionContext) {
        let promptView = ProductPermissionPromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.deliver(.deny)
        }
    }
}
