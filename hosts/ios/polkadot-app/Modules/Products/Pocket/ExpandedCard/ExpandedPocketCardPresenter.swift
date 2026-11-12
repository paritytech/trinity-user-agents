import Foundation

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

extension ExpandedPocketCardPresenter: ExpandedPocketCardInteractorOutputProtocol {}
