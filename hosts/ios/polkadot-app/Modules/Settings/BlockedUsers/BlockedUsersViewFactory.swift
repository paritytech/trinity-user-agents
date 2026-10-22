import Foundation
import Operation_iOS

@MainActor
enum BlockedUsersViewFactory {
    static func createView() -> BlockedUsersViewProtocol? {
        // The chat product runs the person chats on this install: it keeps the blocked contacts (HX2).
        let productChat = PersonChatMode.isProductChat
        let interactor = BlockedUsersInteractor(
            chatContactDataProviderFactory: ChatContactDataProviderFactory(),
            blockUserService: productChat ? ProductContactBlocking() : BlockUserService(),
            productChat: productChat
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
