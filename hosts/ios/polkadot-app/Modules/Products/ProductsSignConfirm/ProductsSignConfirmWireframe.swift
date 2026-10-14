import UIKit

/// Wireframe for the rust-core sign confirmation. Owns the confirmation
/// `context` so the dismiss completion is the single place that resolves the
/// awaiting core call (``ProductsSignConfirmContextProtocol/deliver(_:)``) —
/// guaranteeing the core learns the result only after the sheet is gone.
@MainActor
final class ProductsSignConfirmWireframe: ProductsSignConfirmWireframeProtocol {
    private let context: any ProductsSignConfirmContextProtocol

    init(context: any ProductsSignConfirmContextProtocol) {
        self.context = context
    }

    func hide(view: PolkadotSigningViewProtocol?, decision: Bool) {
        guard let controller = view?.controller else {
            context.deliver(decision)
            return
        }
        controller.dismiss(animated: true) { [context] in
            MainActor.assumeIsolated { context.deliver(decision) }
        }
    }
}
