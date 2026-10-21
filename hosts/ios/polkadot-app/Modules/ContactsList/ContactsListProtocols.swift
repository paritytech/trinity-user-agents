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

/// What the user can do with one row of the chat list: the mute and the archive actions of a product room
/// (P10.10). A person chat has none.
struct ChatListRowActions: Equatable {
    let isMuted: Bool
    let isArchived: Bool
}

@MainActor
protocol ContactsListPresenterProtocol: AnyObject {
    func setup()
    func viewWillAppear()
    func viewWillDisappear()
    func showScanPanel()
    func openChat(contactIdentifier: String)
    func showIncomingRequests()
    func showArchivedChats()
    func rowActions(for contactIdentifier: String) -> ChatListRowActions?
    func apply(_ change: ChatRoomUserStateChange, contactIdentifier: String)
    /// Asks the user how long to mute the room, then mutes it.
    func requestMute(contactIdentifier: String)
    func search(text: String)
}

protocol ContactsListInteractorInputProtocol: AnyObject {
    func setup()
    func notifyViewAppeared()
    func notifyViewDisappeared()
    func entryRoute(for model: ChatOpenModel) async throws -> ChatExtensionEntryRoute
    func apply(_ change: ChatRoomUserStateChange, to chatId: Chat.Id)
    /// Searches the names of the chats and the stored messages. An empty text ends the search.
    func search(text: String)
}

@MainActor
protocol ContactsListInteractorOutputProtocol: AnyObject {
    func didReceive(model: ChatListModel)
    func didReceive(networkStatus: NetworkStatus)
    func didReceive(error: Error)
    /// Nil when the search ended.
    func didReceive(searchResults: ChatListSearchResults?)
}

@MainActor
protocol ContactsListWireframeProtocol: AlertPresentable, ErrorPresentable {
    func showScanPanel()
    func showChat(from view: ContactsListViewProtocol?, for model: ChatOpenModel)
    /// Opens a chat and shows one of its messages.
    func showChat(from view: ContactsListViewProtocol?, for model: ChatOpenModel, showing messageId: Chat.MessageId)
    func open(url: URL)
    func showIncomingRequests(from view: ContactsListViewProtocol?)
    func showArchivedChats(from view: ContactsListViewProtocol?)
    func showMuteOptions(from view: ContactsListViewProtocol?, onSelect: @escaping (Chat.MuteDuration) -> Void)
}
