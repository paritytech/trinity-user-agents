import PolkadotUI
import UIKitExt

protocol ChatRequestListViewProtocol: ControllerBackedProtocol {
    func didReceive(items: [ChatRequestListItem])
}

@MainActor
protocol ChatRequestListPresenterProtocol: AnyObject {
    func setup()
    func selectRequest(with id: String)
}

protocol ChatRequestListInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol ChatRequestListInteractorOutputProtocol: AnyObject {
    func didReceiveChats(_ chats: [Chat.LocalModel])
}

@MainActor
protocol ChatRequestListWireframeProtocol: AnyObject {
    func showChat(
        from view: ChatRequestListViewProtocol?,
        openModel: ChatOpenModel
    )
    func close(from view: ChatRequestListViewProtocol?)
}
