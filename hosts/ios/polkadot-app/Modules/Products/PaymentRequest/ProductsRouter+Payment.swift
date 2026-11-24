import Products
import UIKitExt

/// Presents the payment request sheet; delivers a rejection when no view is attached.
extension ProductsRouting {
    @MainActor
    func showPaymentRequest(context: PaymentRequestContext) {
        guard let view = PaymentRequestViewFactory.createView(context: context) else {
            context.deliverRejected()
            return
        }

        if !present(view: view) {
            context.deliverRejected()
        }
    }
}
