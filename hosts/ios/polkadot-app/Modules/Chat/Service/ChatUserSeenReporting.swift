import Foundation

/// A chat extension that takes the messages that the user saw.
///
/// This is the seam for the Chat Extension v2 `Seen` event of a room that the host stores (02-DESIGN §4).
protocol ChatUserSeenHandling {
    /// The user saw these messages of the room now.
    func userSaw(messageIds: Set<Chat.MessageId>, roomId: String?) async
}

extension ChatEngine {
    /// Tells the bot of a product room which messages the user saw.
    func reportSeen(_ messageIds: Set<Chat.MessageId>) async {
        guard case let .chatExtension(extensionId, roomId) = chatId else {
            return
        }

        let handler = chatExtensionRegistry.getChatExtensionBot(for: extensionId) as? ChatUserSeenHandling
        await handler?.userSaw(messageIds: messageIds, roomId: roomId)
    }
}
