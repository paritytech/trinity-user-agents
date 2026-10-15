import Foundation

struct ChatListModel {
    let establishedChats: [ChatWithPeerMetadata]
    let pendingIncomingRequestCount: Int
    let newIncomingRequestCount: Int
    /// The product rooms that the user archived. The main list shows them as one "Archived" row (P10.10).
    var archivedChatCount: Int = 0
}

/// The chats that a chat list shows (07-LATER §6.2).
enum ChatListScope: Equatable {
    /// Every chat that the user did not archive, and one row that opens the archived rooms.
    case main
    /// Only the product rooms that the user archived.
    case archived

    func shows(_ chat: Chat.LocalModel) -> Bool {
        let archived = chat.roomDeclarations?.userState.archived == true
        switch self {
        case .main: return !archived
        case .archived: return archived
        }
    }
}

/// What the search field of the chat list found (07-LATER §8.2, D29): the chats whose name holds the text, and
/// the newest stored messages that hold it. A product-source room matches by its name only, so the search never
/// asks a product and never starts a worker.
struct ChatListSearchResults {
    let query: String
    let chats: [ChatWithPeerMetadata]
    let hits: [ChatListMessageHit]
}

struct ChatListMessageHit {
    let hit: ChatSearchHit
    let chat: ChatWithPeerMetadata

    /// The identifier of the row of the hit in the list.
    var identifier: String {
        "hit:\(chat.chat.identifier)|\(hit.messageId)"
    }
}
