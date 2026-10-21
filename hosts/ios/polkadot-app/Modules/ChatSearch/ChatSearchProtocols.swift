import Foundation
import UIKitExt

/// One row of the results of the in-room search.
struct ChatSearchRowViewModel: Equatable {
    let messageId: Chat.MessageId
    let author: String
    let time: String
    /// The line of the message that holds the match, with the range of the match in it.
    let line: String
    let match: NSRange?
}

enum ChatSearchViewState: Equatable {
    /// The field is empty.
    case idle
    case searching
    case results([ChatSearchRowViewModel], hasMore: Bool)
    case empty
    /// The product cannot search the room now.
    case unavailable
}

@MainActor
protocol ChatSearchViewProtocol: ControllerBackedProtocol {
    func didReceive(state: ChatSearchViewState)
}

@MainActor
protocol ChatSearchPresenterProtocol: AnyObject {
    func setup()
    func search(text: String)
    func loadMore()
    func select(messageId: Chat.MessageId)
}

@MainActor
protocol ChatSearchInteractorInputProtocol: AnyObject {
    func search(text: String)
    func loadMore()
}

@MainActor
protocol ChatSearchInteractorOutputProtocol: AnyObject {
    func didStartSearch()
    /// The hits of the text so far, newest first.
    func didReceive(hits: [ChatSearchHit], hasMore: Bool)
    func didClear()
    func didFail(_ error: ChatRoomSearchError)
}

@MainActor
protocol ChatSearchWireframeProtocol: AnyObject {
    /// Closes the search and shows the message in the room.
    func close(from view: ChatSearchViewProtocol?, showing messageId: Chat.MessageId)
}
