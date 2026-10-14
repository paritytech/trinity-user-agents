import UIKit

final class PolkadotSignInWireframe: PolkadotSignInWireframeProtocol {
    let onResult: ((PolkadotSignInResult) -> Void)?

    init(onResult: ((PolkadotSignInResult) -> Void)?) {
        self.onResult = onResult
    }

    func hide(view: PolkadotSignInViewProtocol?, with result: PolkadotSignInResult?) {
        view?.controller.dismiss(animated: true) { [onResult] in
            guard let result else { return }
            onResult?(result)
        }
    }
}
