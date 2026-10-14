import Foundation

@MainActor
final class RecoveryWarningWireframe: RecoveryWarningWireframeProtocol {
    let action: () -> Void
    init(action: @escaping () -> Void) {
        self.action = action
    }

    func hide(view: RecoveryWarningViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }

    func hideWithAction(view: (any RecoveryWarningViewProtocol)?) {
        view?.controller.dismiss(animated: true, completion: action)
    }
}
