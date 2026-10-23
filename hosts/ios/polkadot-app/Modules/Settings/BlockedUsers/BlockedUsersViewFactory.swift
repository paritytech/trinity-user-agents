import Foundation
import Operation_iOS

@MainActor
enum BlockedUsersViewFactory {
    static func createView() -> BlockedUsersViewProtocol? {
        // The chat product runs the person chats: it keeps the blocked contacts (HX2).
        let interactor = BlockedUsersInteractor(
            chatContactDataProviderFactory: ChatContactDataProviderFactory(),
            blockUserService: ProductContactBlocking()
        )

        let wireframe = BlockedUsersWireframe()

        let presenter = BlockedUsersPresenter(
            interactor: interactor,
            wireframe: wireframe
        )

        let view = BlockedUsersViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
