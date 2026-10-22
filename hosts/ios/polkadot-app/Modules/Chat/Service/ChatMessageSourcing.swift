import Foundation
import AsyncExtensions

/// Where the room screen reads its messages and where it writes what the user does (02-DESIGN §8.3).
///
/// `CoreDataMessageSource` reads and writes the host database. `ProductMessageSource` asks the product for
/// pages and sends the actions of the user to the product.
protocol ChatMessageSourcing: AnyObject, Sendable {
    /// The window of messages, oldest first. A new value comes each time the window changes.
    func snapshots() async throws -> AnyAsyncSequence<MessageListSnapshot>

    /// Asks for the page before the oldest message of the window.
    func loadOlder() async -> ChatPageOutcome

    /// Asks for the page after the newest message of the window.
    func loadNewer() async -> ChatPageOutcome

    /// Asks for pages until the message is in the window. Gives false when both ends of the room come first.
    func jump(to messageId: Chat.MessageId) async -> Bool

    /// Writes an action of the user.
    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome

    /// Asks for the newest page again, for example after the room became unavailable.
    func reload() async
}

/// The window of messages that the room screen shows.
struct MessageListSnapshot: Equatable {
    /// The messages, oldest first.
    let messages: [Chat.LocalMessage]
    let paging: ChatPagingState
    let readiness: ChatRoomReadiness
    /// The edit time of each edited message, in milliseconds since the Unix epoch.
    let editedAt: [Chat.MessageId: Chat.Timestamp]
    /// The messages that replies of the window quote and the window does not hold, by their ids.
    let replyPreviews: [Chat.MessageId: Chat.LocalMessage]
    /// The messages with a reaction of the contact that the user has not seen yet, oldest first.
    let unseenReactionTargets: [Chat.MessageId]

    init(
        messages: [Chat.LocalMessage],
        paging: ChatPagingState = .complete,
        readiness: ChatRoomReadiness = .ready,
        editedAt: [Chat.MessageId: Chat.Timestamp] = [:],
        replyPreviews: [Chat.MessageId: Chat.LocalMessage] = [:],
        unseenReactionTargets: [Chat.MessageId] = []
    ) {
        self.messages = messages
        self.paging = paging
        self.readiness = readiness
        self.editedAt = editedAt
        self.replyPreviews = replyPreviews
        self.unseenReactionTargets = unseenReactionTargets
    }
}

/// What the room screen can ask at each end of the window.
struct ChatPagingState: Equatable {
    enum Edge: Equatable {
        /// More messages exist past this end.
        case more
        /// A page for this end is on its way.
        case loading
        /// No more messages exist past this end.
        case end
        /// The last page for this end did not arrive. The room screen can ask again.
        case failed
    }

    let older: Edge
    let newer: Edge

    /// A window that holds the whole room, as the host database gives it.
    static let complete = ChatPagingState(older: .end, newer: .end)
}

/// Whether the room can answer the room screen (P5.7).
enum ChatRoomReadiness: Equatable {
    /// The first page is on its way.
    case loading
    case ready
    /// The product did not answer. The window keeps the cached pages.
    case unavailable
}

enum ChatPageOutcome: Equatable {
    /// The page added this number of messages to the window. Zero when a page for this end is on its way.
    case appended(Int)
    /// No more messages exist past this end.
    case end
    /// The page did not arrive in time, or the product failed to give it.
    case failed
    /// The product is not available.
    case unavailable
}

/// An action of the user on the messages of a room.
enum ChatSourceAction: Equatable {
    /// A new message, a reply, an edit or a reaction, as the host database keeps them.
    case send(Chat.LocalMessage.Content)
    /// The user saw these messages.
    case markSeen(Set<Chat.MessageId>)
    /// Adds the reaction of the user, or removes it when the user has it.
    case toggleReaction(messageId: Chat.MessageId, emoji: String)
    /// What the user did with the contact of a room of the chat product.
    case contactAction(Chat.ContactAction)
}

extension Chat {
    /// What the user does with the contact of a room, as the native chat offers it: the answer to a chat request,
    /// a block, an unblock, or a leave.
    enum ContactAction: Equatable {
        case acceptRequest
        case declineRequest
        case block
        case unblock
        case leave
    }
}

enum ChatSourceActionOutcome: Equatable {
    case done
    /// The product refused the action, and the window shows the state before it. The reason is for the user.
    case refused(reason: String?)
    /// The message stays in the window with the failed status.
    case failed
}

enum ChatMessageSourceError: Error, Equatable {
    /// This source does not do the action.
    case unsupported
}

/// The product refused an action of the user. The room screen shows the reason.
struct ChatUserActionRefusal: Error, Equatable {
    let reason: String?
}
