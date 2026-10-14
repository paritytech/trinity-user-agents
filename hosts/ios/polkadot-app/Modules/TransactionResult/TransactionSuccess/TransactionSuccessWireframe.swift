import Foundation

final class TransactionSuccessWireframe: TransactionSuccessWireframeProtocol {
    let onHide: TransactionSuccessCompletion?

    init(onHide: TransactionSuccessCompletion?) {
        self.onHide = onHide
    }

    func hide(view: TransactionSuccessViewProtocol?) {
        view?.controller.dismiss(animated: true, completion: onHide)
    }
}
