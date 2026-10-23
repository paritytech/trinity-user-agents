import Foundation
import TrUAPIHost

/// The product rooms that take a shared text, and the way the text goes to them.
protocol ShareProductRoomsProviding: AnyObject, Sendable {
    /// The chat as the share sheet lists it, or `nil` when the room cannot take a shared text.
    func target(for chat: Chat.LocalModel) -> ChatWithPeerMetadata?

    /// Sends `text` to the room `chatId` as a message of the user.
    func send(text: String, to chatId: Chat.Id) async throws
}

enum ShareProductRoomError: Error, Equatable {
    /// The chat is no room of the chat product.
    case noRoom
    /// A product room takes a shared text, and nothing else.
    case unsupportedContent
}

/// A room is a target when it is the conversation with a contact of the chat product that both sides accepted, as
/// the native chat lists its person chats that are ready for messaging. The text goes to the outbox of the room, as
/// a message that the user types there, and the outbox gives it to the product, as the native chat keeps a shared
/// text in its outbox.
final class ShareProductRooms: ShareProductRoomsProviding, @unchecked Sendable {
    private let registry: ChatExtensionsRegistering
    private let outbox: ProductOutbox

    init(registry: ChatExtensionsRegistering, outbox: ProductOutbox = .shared) {
        self.registry = registry
        self.outbox = outbox
    }

    func target(for chat: Chat.LocalModel) -> ChatWithPeerMetadata? {
        guard
            case let .chatExtension(extensionId, roomId) = chat.peer,
            roomId != nil,
            registry.hasChatExtension(for: extensionId),
            let policy = chat.roomPolicy,
            policy.messageStore == .product,
            policy.footer.showsInput,
            let contact = policy.contact,
            contact.state == .established || contact.state == .peerLeft
        else {
            return nil
        }

        return chat.chatWithPeerMetadata(using: registry)
    }

    func send(text: String, to chatId: Chat.Id) async throws {
        guard case let .chatExtension(extensionId, roomId?) = chatId else {
            throw ShareProductRoomError.noRoom
        }

        try await outbox.send(text: text, replyTo: nil, productId: extensionId, roomId: roomId)
    }
}
