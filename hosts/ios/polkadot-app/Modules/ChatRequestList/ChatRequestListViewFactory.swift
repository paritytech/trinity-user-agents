import Foundation
import Operation_iOS
import Keystore_iOS

@MainActor
enum ChatRequestListViewFactory {
    static func createView(for flowState: ChatFlowState) -> ChatRequestListViewProtocol? {
        let interactor = ChatRequestListInteractor(
            chatsProvider: ChatContactDataProviderFactory(),
            logger: Logger.shared
        )

        let wireframe = ChatRequestListWireframe(flowState: flowState)

        let presenter = ChatRequestListPresenter(interactor: interactor, wireframe: wireframe)

        let view = ChatRequestListViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
