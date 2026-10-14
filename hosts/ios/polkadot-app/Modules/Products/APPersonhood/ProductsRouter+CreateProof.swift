import Products
import UIKitExt

/// Presents the create-proof confirmation; delivers a rejection when no view is attached.
extension ProductsRouting {
    func showCreateProofPrompt(context: CreateProofConfirmationContext) {
        let promptView = CreateProofPromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.deliver(.rejected)
        }
    }
}
