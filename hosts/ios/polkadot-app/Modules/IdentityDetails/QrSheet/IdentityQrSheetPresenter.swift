import Foundation

@MainActor
final class IdentityQrSheetPresenter {
    weak var view: IdentityQrSheetViewProtocol?

    private let wireframe: IdentityQrSheetWireframeProtocol

    init(wireframe: IdentityQrSheetWireframeProtocol) {
        self.wireframe = wireframe
    }
}

extension IdentityQrSheetPresenter: IdentityQrSheetPresenterProtocol {
    func setup() {}

    func close() {
        wireframe.close(from: view)
    }
}
