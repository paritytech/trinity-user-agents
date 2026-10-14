import UIKit

final class TransferPrivacyWireframe: TransferPrivacyWireframeProtocol {
    func complete(from view: TransferPrivacyViewProtocol?, _ completion: (() -> Void)?) {
        view?.controller.dismiss(animated: true, completion: completion)
    }
}
