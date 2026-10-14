import Foundation

@MainActor
final class TopUpMismatchWireframe: TopUpMismatchWireframeProtocol {
    func dismiss(view: TopUpMismatchViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }
}
