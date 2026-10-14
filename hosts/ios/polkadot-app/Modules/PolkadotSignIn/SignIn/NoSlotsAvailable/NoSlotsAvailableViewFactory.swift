import Foundation

@MainActor
enum NoSlotsAvailableViewFactory {
    static func createView(message: String) -> NoSlotsAvailableViewController {
        let wireframe = NoSlotsAvailableWireframe()
        let presenter = NoSlotsAvailablePresenter(message: message, wireframe: wireframe)
        let viewController = NoSlotsAvailableViewController(presenter: presenter)
        presenter.view = viewController
        BottomSheetViewFacade.setupBottomSheet(from: viewController)
        return viewController
    }
}
