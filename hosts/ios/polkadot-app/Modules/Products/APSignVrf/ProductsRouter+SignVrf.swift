import Products
import UIKitExt

/// Presents the sign-vrf confirmation; delivers a rejection when no view is attached.
extension ProductsRouting {
    func showSignVrfPrompt(context: SignVrfConfirmationContext) {
        let promptView = SignVrfPromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.deliver(.rejected)
        }
    }
}
