import Foundation
import UIKit
import UIKitExt

@MainActor
final class ClaimLiteUsernameWireframe: ClaimUsernameWireframeProtocol {
    let observer: RootStateObserving

    init(observer: RootStateObserving) {
        self.observer = observer
    }

    func finishFlow(from _: ControllerBackedProtocol?) {
        observer.didClaimUsername()
    }

    func showRecovery(from view: ControllerBackedProtocol?) {
        guard
            let optionView = AccountRecoveryViewFactory.createView(observer: observer)
        else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            optionView.controller,
            animated: true
        )
    }
}
