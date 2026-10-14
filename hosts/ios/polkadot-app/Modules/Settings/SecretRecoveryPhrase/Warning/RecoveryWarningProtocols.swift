import Combine
import UIKitExt

protocol RecoveryWarningViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModels: [RecoveryWarningViewLayout.Model])
}

@MainActor
protocol RecoveryWarningPresenterProtocol: AnyObject {
    func setup()
    func onClose()
    func onAction()
}

protocol RecoveryWarningInteractorInputProtocol: AnyObject {}

@MainActor
protocol RecoveryWarningInteractorOutputProtocol: AnyObject {}

@MainActor
protocol RecoveryWarningWireframeProtocol: AnyObject, AlertPresentable, CommonRetryable {
    func hide(view: RecoveryWarningViewProtocol?)
    func hideWithAction(view: RecoveryWarningViewProtocol?)
}
