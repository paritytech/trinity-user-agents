import Products
import UIKitExt

/// Product signing presentation. `showSigning` drives the full flow (signs and
/// replies through the context) and reports whether the sheet was shown;
/// `showSignConfirmation` only resolves a rust-core decision and delivers a
/// rejection when no view is attached.
extension ProductsRouting {
    @discardableResult
    func showSigning(with context: PolkadotSigningContextProtocol) -> Bool {
        guard let signingView = PolkadotSigningViewFactory.createView(signingContext: context) else {
            return false
        }

        return present(view: signingView)
    }

    func showSignConfirmation(with context: any ProductsSignConfirmContextProtocol) {
        let confirmationView = ProductsSignConfirmViewFactory.createView(context: context)

        if !present(view: confirmationView) {
            context.deliver(false)
        }
    }
}
