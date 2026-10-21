import Foundation
import TrUAPIHost

/// The product rooms that take a shared text, and the way the text goes to them.
protocol ShareProductRoomsProviding: AnyObject, Sendable {
    /// The chat as the share sheet lists it, or `nil` when the room cannot take a shared text.
    func target(for chat: Chat.LocalModel) -> ChatWithPeerMetadata?

    /// Sends `text` to the room `chatId` as a message of the user.
    func send(text: String, to chatId: Chat.Id) async throws
}

/// The product cannot take the shared text.
enum ShareProductRoomError: Error, Equatable {
    case noRoom
    case productNotRunning
    case refused(reason: String)
}

/// A room is a target when it keeps its messages in its product, shows the input, is no thread, and its product
/// runs. The text goes through the `NewMessage` user action of the room, as a message that the user types there,
/// so the product sends it and keeps it.
///
/// A room that the host stores is not a target: its product gets a message there only from the room screen.
final class ShareProductRooms: ShareProductRoomsProviding, @unchecked Sendable {
    private let registry: ChatExtensionsRegistering

    init(registry: ChatExtensionsRegistering) {
        self.registry = registry
    }

    func target(for chat: Chat.LocalModel) -> ChatWithPeerMetadata? {
        guard
            case let .chatExtension(extensionId, roomId) = chat.peer,
            roomId != nil,
            registry.hasChatExtension(for: extensionId),
            let policy = chat.roomPolicy,
            policy.messageStore == .product,
            policy.footer.showsInput
        else {
            return nil
        }

        return chat.chatWithPeerMetadata(using: registry)
    }

    func send(text: String, to chatId: Chat.Id) async throws {
        guard case let .chatExtension(extensionId, roomId?) = chatId else {
            throw ShareProductRoomError.noRoom
        }
        guard let querying = registry.getChatExtensionBot(for: extensionId) as? ProductChatQuerying else {
            throw ShareProductRoomError.productNotRunning
        }

        do {
            _ = try await querying.userAction(ProductChatUserActionRequest(
                roomId: roomId,
                action: .newMessage(body: .text(text: text), replyTo: nil)
            ))
        } catch let ChatQueryError.actionRefused(.rejected(reason)) {
            throw ShareProductRoomError.refused(reason: reason)
        }
    }
}
