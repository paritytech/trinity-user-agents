import Products
import UIKitExt

/// Presents the statement-sign confirmation; delivers a rejection when no view is attached.
extension ProductsRouting {
    func showStatementSignPrompt(context: StatementSignConfirmationContext) {
        let promptView = StatementSignPromptViewFactory.createView(context: context)

        if !present(view: promptView) {
            context.deliver(.rejected)
        }
    }
}
