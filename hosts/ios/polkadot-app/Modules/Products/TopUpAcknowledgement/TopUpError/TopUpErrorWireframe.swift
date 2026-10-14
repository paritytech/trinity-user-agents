import Foundation

@MainActor
final class TopUpErrorWireframe: TopUpErrorWireframeProtocol {
    func dismiss(view: TopUpErrorViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }
}
