import Foundation
import UIKit
import UIKitExt

@MainActor
final class SuccessPaymentWireframe: TransferAmountWireframeProtocol {
    func hide(view: ControllerBackedProtocol?) {
        presentTransactionSuccess(from: view) { [weak view] in
            view?.controller.presentingViewController?.dismiss(animated: true)
        }
    }
}
