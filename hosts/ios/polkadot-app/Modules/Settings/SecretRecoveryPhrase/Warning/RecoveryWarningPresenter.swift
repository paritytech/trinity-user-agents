import Foundation
import Combine
import UIKit
import BigInt

final class RecoveryWarningPresenter {
    weak var view: RecoveryWarningViewProtocol?
    let wireframe: RecoveryWarningWireframeProtocol
    let interactor: RecoveryWarningInteractorInputProtocol

    init(
        interactor: RecoveryWarningInteractorInputProtocol,
        wireframe: RecoveryWarningWireframeProtocol,
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension RecoveryWarningPresenter: RecoveryWarningPresenterProtocol {
    func setup() {
        reload()
    }

    func onClose() {
        wireframe.hide(view: view)
    }

    func onAction() {
        wireframe.hideWithAction(view: view)
    }
}

extension RecoveryWarningPresenter {
    private func reload() {
        let texts = [
            String(localized: .secretWarningText1),
            String(localized: .secretWarningText2),
            String(localized: .secretWarningText3)
        ]
        let images = [
            UIImage.iconWarning,
            UIImage.iconLock,
            UIImage.iconEye
        ]

        let models = zip(images, texts).map {
            RecoveryWarningViewLayout.Model(icon: $0.0, text: $0.1)
        }
        view?.didReceive(viewModels: models)
    }
}

extension RecoveryWarningPresenter: RecoveryWarningInteractorOutputProtocol {}
