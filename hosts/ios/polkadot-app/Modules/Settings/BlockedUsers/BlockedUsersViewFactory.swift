import Foundation
import Operation_iOS

@MainActor
enum BlockedUsersViewFactory {
    static func createView(allowanceCleanupService: AllowanceCleanupServicing) -> BlockedUsersViewProtocol? {
        let interactor = BlockedUsersInteractor(
            chatContactDataProviderFactory: ChatContactDataProviderFactory(),
            blockUserService: BlockUserService(allowanceCleanupService: allowanceCleanupService)
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
