import UIKit

final class TransactionSuccessPresenter {
    weak var view: TransactionSuccessViewProtocol?
    let wireframe: TransactionSuccessWireframeProtocol

    let generator = UINotificationFeedbackGenerator()

    init(wireframe: TransactionSuccessWireframeProtocol) {
        self.wireframe = wireframe
    }
}

extension TransactionSuccessPresenter: TransactionSuccessPresenterProtocol {
    func setup() {
        generator.prepare()
    }

    func onAppear() {
        generator.notificationOccurred(.success)
    }

    func activateDone() {
        wireframe.hide(view: view)
    }
}
