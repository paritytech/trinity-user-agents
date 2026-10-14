import Foundation

final class NoSlotsAvailableWireframe: NoSlotsAvailableWireframeProtocol {
    func close(view: NoSlotsAvailableViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }
}
