import Foundation

final class RemoveDeviceWireframe: RemoveDeviceWireframeProtocol {
    func close(view: RemoveDeviceViewProtocol?, completion: (() -> Void)?) {
        view?.controller.dismiss(animated: true, completion: completion)
    }
}
