import Foundation

extension Chat.LocalMessage {
    static func newExtensionMessage(
        _ extensionId: String,
        roomId: String? = nil,
        content: Chat.LocalMessage.Content
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: .chatExtension(extensionId, roomId: roomId),
            origin: .chatExtension(extensionId),
            creationSource: .localDevice,
            status: .incoming(.new),
            timestamp: Date().toChatTimestamp(),
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}
