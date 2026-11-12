import Foundation
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome

@MainActor
final class ExpandedPocketCardPresenter {
    weak var view: ExpandedPocketCardViewProtocol?
    let wireframe: ExpandedPocketCardWireframeProtocol
    let interactor: ExpandedPocketCardInteractorInputProtocol

    init(
        interactor: ExpandedPocketCardInteractorInputProtocol,
        wireframe: ExpandedPocketCardWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension ExpandedPocketCardPresenter: ExpandedPocketCardPresenterProtocol {
    func setup() {}
}

extension ExpandedPocketCardPresenter: ExpandedPocketCardInteractorOutputProtocol {
    func didRequestNavigation(to _: URL) {}

    func didRequestReload() {}

    func didUpdateLoadPhase(_: ProductLoadPhase) {}

    func didFail(error _: Error) {}

    func didRequestFaceShown(_: Bool) -> ExpandedCardFaceOutcome {
        .notPresented
    }
}
