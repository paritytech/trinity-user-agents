import Foundation
import TrUAPIHost

// Maps a feed update of the bindings to the values that the chat store writes. Each body goes into the
// content kind of the native chat that draws it (see `ProductChatMessage+Local.swift`).

extension Chat.ProductFeedUpdate {
    init(_ update: ChatFeedUpdate) {
        switch update {
        case let .new(message):
            self = .new(Chat.ProductFeedMessage(message))
        case let .edited(messageId, body, editedAt):
            self = .edited(messageId: messageId, content: body.nativeContent(replyTo: nil), editedAt: editedAt)
        case let .reactionPlaced(messageId, participantId, emoji):
            self = .reactionPlaced(messageId: messageId, participantId: participantId, emoji: emoji)
        case let .reactionRemoved(messageId, participantId, emoji):
            self = .reactionRemoved(messageId: messageId, participantId: participantId, emoji: emoji)
        case let .statusChanged(messageId, status):
            self = .statusChanged(messageId: messageId, status: Chat.ProductMessageStatus(status))
        case let .replaced(message):
            self = .replaced(Chat.ProductFeedMessage(message))
        }
    }
}

extension Chat.ProductFeedMessage {
    /// The store resolves the reply against the messages of the room, so the content carries no quote yet.
    init(_ message: ChatMessage) {
        var body = message
        body.replyTo = nil

        self.init(
            messageId: message.messageId,
            participantId: message.origin.participantId,
            timestamp: message.timestamp,
            content: body.nativeContent,
            replyTo: message.replyTo
        )
    }
}

extension Chat.ProductMessageStatus {
    init(_ status: ChatMessageStatus) {
        switch status {
        case .preparing: self = .preparing
        case .sent: self = .sent
        case .delivered: self = .delivered
        case .read: self = .read
        case .failed: self = .failed
        }
    }
}

extension Chat.ProductFeedError {
    /// The rejection that the core gives to the product for this refusal.
    var rejection: NativeChatFeedRejection {
        switch self {
        case .unknownMessage: .UnknownMessage
        case .duplicateMessage: .DuplicateMessage
        case .notOwner: .NotOwner
        }
    }
}
