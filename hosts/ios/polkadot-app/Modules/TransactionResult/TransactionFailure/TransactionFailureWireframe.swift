import Foundation

final class TransactionFailureWireframe: TransactionFailureWireframeProtocol {
    let onHide: TransactionFailureCompletion?

    init(onHide: TransactionFailureCompletion?) {
        self.onHide = onHide
    }

    func hide(view: TransactionFailureViewProtocol?) {
        view?.controller.dismiss(animated: true, completion: onHide)
    }
}
