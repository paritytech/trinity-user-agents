import Products
import UIKit
import UIKitExt

/// Presents the contact picker; delivers a dismissal when no view is attached.
///
/// The same screen the user adds a chat contact from, so choosing a person for
/// a product looks like choosing one anywhere else in the app, and the names
/// are drawn by code the host already owns.
extension ProductsRouting {
    @MainActor
    func showContactPick(context: ContactPickContext) {
        guard let view = ContactPickViewFactory.createView(context: context) else {
            context.deliver(nil)
            return
        }

        view.controller.modalPresentationStyle = .fullScreen
        view.controller.modalTransitionStyle = .crossDissolve

        if !present(view: view) {
            context.deliver(nil)
        }
    }
}
