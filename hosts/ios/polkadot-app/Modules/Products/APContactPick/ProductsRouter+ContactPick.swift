import Products
import UIKit
import UIKitExt

/// Presents the contact picker and returns it; delivers a dismissal when no
/// view is attached.
///
/// The same screen the user adds a chat contact from, so choosing a person for
/// a product looks like choosing one anywhere else in the app, and the names
/// are drawn by code the host already owns.
extension ProductsRouting {
    @MainActor
    func showContactPick(context: ContactPickContext) -> UIViewController? {
        guard let view = ContactPickViewFactory.createView(context: context) else {
            context.deliver(nil)
            return nil
        }

        view.controller.modalPresentationStyle = .fullScreen
        view.controller.modalTransitionStyle = .crossDissolve

        guard present(view: view) else {
            context.deliver(nil)
            return nil
        }
        return view.controller
    }
}
