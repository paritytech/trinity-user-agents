import Foundation

@MainActor
enum ChatSearchViewFactory {
    /// The search of one room. `showMessage` shows a message of the room when the user picks a result.
    static func createView(
        source: ChatRoomSearching,
        roomId: String,
        peerName: String,
        showMessage: @escaping (Chat.MessageId) -> Void
    ) -> ChatSearchViewProtocol {
        let interactor = ChatSearchInteractor(source: source)
        let wireframe = ChatSearchWireframe(showMessage: showMessage)
        let presenter = ChatSearchPresenter(
            interactor: interactor,
            wireframe: wireframe,
            roomId: roomId,
            peerName: peerName
        )
        let view = ChatSearchViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
