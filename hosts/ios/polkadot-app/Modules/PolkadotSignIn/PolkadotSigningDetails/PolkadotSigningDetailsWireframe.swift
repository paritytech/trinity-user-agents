import UIKit

final class PolkadotSigningDetailsWireframe: PolkadotSigningDetailsWireframeProtocol {
    func hide(view: PolkadotSigningDetailsViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }
}
