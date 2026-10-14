import Foundation

/// Shows that a top-up settled without crediting anything. Informational — closing dismisses.
final class TopUpErrorPresenter {
    weak var view: TopUpErrorViewProtocol?
    let wireframe: TopUpErrorWireframeProtocol

    private let title: String
    private let message: String
    private let closeButtonTitle: String

    init(
        wireframe: TopUpErrorWireframeProtocol,
        title: String,
        message: String,
        closeButtonTitle: String
    ) {
        self.wireframe = wireframe
        self.title = title
        self.message = message
        self.closeButtonTitle = closeButtonTitle
    }
}

extension TopUpErrorPresenter: TopUpErrorPresenterProtocol {
    func setup() {
        view?.didReceive(
            title: title,
            message: message,
            closeButtonTitle: closeButtonTitle
        )
    }

    func didTapClose() {
        wireframe.dismiss(view: view)
    }
}
