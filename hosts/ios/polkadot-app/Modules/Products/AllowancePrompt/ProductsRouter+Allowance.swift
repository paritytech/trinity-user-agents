import Products
import UIKitExt

/// Presents the resource allocation approval bottom sheet above the host
/// product view controller; delivers a rejection when no view is attached.
extension ProductsRouter: @retroactive AllowancePromptRouting {
    public func showAllowancePrompt(context: AllowancePromptContext) {
        let promptView = AllowancePromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.deliver(.rejected)
        }
    }
}
