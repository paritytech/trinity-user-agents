import Foundation

@MainActor
final class NoSlotsAvailablePresenter: NoSlotsAvailablePresenterProtocol {
    weak var view: NoSlotsAvailableViewProtocol?
    let wireframe: NoSlotsAvailableWireframeProtocol

    private let message: String

    init(message: String, wireframe: NoSlotsAvailableWireframeProtocol) {
        self.message = message
        self.wireframe = wireframe
    }

    func setup() {
        view?.didReceive(message: message)
    }

    func dismiss() {
        wireframe.close(view: view)
    }
}
