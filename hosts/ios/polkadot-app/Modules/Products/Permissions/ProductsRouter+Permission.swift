import Products
import UIKitExt

/// Presents ``ProductPermissionPrompt`` as a bottom sheet above the host
/// product view controller. A prompt that cannot be shown is not a decision:
/// the core keeps the question open instead of storing a denial.
extension ProductsRouter: @retroactive ProductPermissionRouting {
    public func showPrompt(context: ProductPermissionContext) {
        let promptView = ProductPermissionPromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.fail(ProductPermissionPromptError.notPresentable)
        }
    }
}
