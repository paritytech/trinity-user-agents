import Foundation
import PolkadotUI
import UIKitExt
import ChainRegistry

protocol ContactsListViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: ContactsListViewLayout.ViewModel)
    func didReceive(titleViewModel: NetworkStatusTitleView.ViewModel)
    /// A tapped chat is waiting for its bot; the view shows that until `didStopOpeningChat`.
    func didStartOpeningChat()
    func didStopOpeningChat()
}

@MainActor
protocol ContactsListPresenterProtocol: AnyObject {
    func setup()
    func viewWillAppear()
    func viewWillDisappear()
    func showScanPanel()
    func openChat(contactIdentifier: String)
    func showIncomingRequests()
}

protocol ContactsListInteractorInputProtocol: AnyObject {
    func setup()
    func notifyViewAppeared()
    func notifyViewDisappeared()
    func entryRoute(for model: ChatOpenModel) async throws -> ChatExtensionEntryRoute
}

@MainActor
protocol ContactsListInteractorOutputProtocol: AnyObject {
    func didReceive(model: ChatListModel)
    func didReceive(networkStatus: NetworkStatus)
    func didReceive(error: Error)
}

@MainActor
protocol ContactsListWireframeProtocol: AlertPresentable, ErrorPresentable {
    func showScanPanel()
    func showChat(from view: ContactsListViewProtocol?, for model: ChatOpenModel)
    func open(url: URL)
    func showIncomingRequests(from view: ContactsListViewProtocol?)
}
