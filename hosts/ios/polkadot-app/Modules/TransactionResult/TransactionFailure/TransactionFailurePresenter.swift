import UIKit

final class TransactionFailurePresenter {
    weak var view: TransactionFailureViewProtocol?
    let wireframe: TransactionFailureWireframeProtocol

    let generator = UINotificationFeedbackGenerator()

    init(wireframe: TransactionFailureWireframeProtocol) {
        self.wireframe = wireframe
    }
}

extension TransactionFailurePresenter: TransactionFailurePresenterProtocol {
    func setup() {
        generator.prepare()
    }

    func onAppear() {
        generator.notificationOccurred(.error)
    }

    func onAction() {
        wireframe.hide(view: view)
    }
}
