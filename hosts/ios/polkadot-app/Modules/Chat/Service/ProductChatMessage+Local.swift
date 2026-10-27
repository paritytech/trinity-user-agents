import Foundation
import TrUAPIHost

/// The participant id that names the user of the host in a product room.
let productUserParticipantId = "user"

// A message of a product room. The room screen and the chat list then draw it with the views and the strings of the
// native person chat: a text with Markdown and links, or an info row.

extension ChatMessage {
    /// The message as the room screen draws it.
    ///
    /// - Parameters:
    ///   - chatId: The chat of the room.
    ///   - extensionId: The extension of the product. Messages of other participants come from it.
    ///   - isSeen: True when the user saw the message on this device and the product has not marked it read yet.
    func localMessage(
        chatId: Chat.Id,
        extensionId: ChatExtension.Id,
        isSeen: Bool = false
    ) -> Chat.LocalMessage {
        let localStatus: Chat.LocalMessage.Status = isFromUser
            ? .outgoing(status.outgoingStatus)
            : .incoming(isUnreadByUser && !isSeen ? .new : .seen)

        return Chat.LocalMessage(
            messageId: messageId,
            chatId: chatId,
            origin: isFromUser ? .user : .chatExtension(extensionId),
            creationSource: .localDevice,
            status: localStatus,
            timestamp: timestamp,
            content: nativeContent,
            reactions: localReactions(chatId: chatId, extensionId: extensionId),
            compactionId: nil,
            relatedMessages: []
        )
    }

    /// The message as the chat list draws it in the preview line of the room.
    func previewMessage() -> Chat.RoomPreviewMessage {
        let localStatus: Chat.LocalMessage.Status = isFromUser
            ? .outgoing(status.outgoingStatus)
            : .incoming(isUnreadByUser ? .new : .seen)

        return Chat.RoomPreviewMessage(
            messageId: messageId,
            status: localStatus,
            timestamp: timestamp,
            content: nativeContent,
            relatedMessages: editRelatedMessages(status: localStatus)
        )
    }

    var isFromUser: Bool {
        origin.participantId == productUserParticipantId
    }

    /// A message of the contact that the user has not seen: the product gives it `Delivered` until the user sees
    /// it and `Read` after.
    var isUnreadByUser: Bool {
        !isFromUser && status == .delivered
    }

    /// The content of the native chat.
    var nativeContent: Chat.LocalMessage.Content {
        body.nativeContent(replyTo: replyTo)
    }

    /// The last edit, which the native preview line shows as "Edited: <text>".
    private func editRelatedMessages(status: Chat.LocalMessage.Status) -> [Chat.RelatedLocalMessage] {
        guard let editedAt, let text = body.plainText else {
            return []
        }

        let edit = Chat.RelatedLocalMessage(
            messageId: messageId + "/edit",
            timestamp: editedAt,
            content: .edited(.init(messageId: messageId, newContent: .init(text: text, attachments: nil))),
            status: status
        )

        return [edit]
    }

    /// One reaction for each count of a summary. The user reaction has the user origin. The product gives no time
    /// for a reaction in the summary, so each reaction takes the time of the message plus its place in the
    /// summaries: the native chat then orders the emojis of the same count as the product does.
    private func localReactions(chatId: Chat.Id, extensionId: ChatExtension.Id) -> [Chat.MessageReaction] {
        let origins = reactions.flatMap { summary in
            let total = Int(summary.count)
            let others = summary.reactedByUser ? max(total - 1, 0) : total
            let userReaction: [Chat.LocalMessage.Origin] = summary.reactedByUser ? [.user] : []
            let productReactions = Array(repeating: Chat.LocalMessage.Origin.chatExtension(extensionId), count: others)
            return (userReaction + productReactions).map { (emoji: summary.emoji, origin: $0) }
        }

        return origins.enumerated().map { offset, reaction in
            Chat.MessageReaction(
                messageId: messageId,
                emoji: reaction.emoji,
                origin: reaction.origin,
                chatId: chatId,
                timestamp: timestamp + UInt64(offset)
            )
        }
    }
}

extension ChatMessageStatus {
    /// The tick of an outgoing message. `Preparing` is a message on its way. The native chat has no read tick,
    /// so it draws `Read` as delivered.
    var outgoingStatus: Chat.LocalMessage.Status.OutgoingStatus {
        switch self {
        case .preparing: .new
        case .sent: .sent
        case .delivered,
             .read: .delivered
        case .failed: .failed
        }
    }
}

extension ChatMessageBody {
    /// The text of a text body or the caption of a media body.
    var plainText: String? {
        switch self {
        case let .text(text): text
        case let .richText(text, _): text
        case .event,
             .call,
             .payment: nil
        }
    }

    /// The body as a content kind of the native chat.
    ///
    /// A reply quote exists for a text body and a media body. This build draws the caption of a media body, and
    /// draws a media body without a caption, a call and a payment as a message that the host does not support.
    func nativeContent(replyTo: String?) -> Chat.LocalMessage.Content {
        switch self {
        case let .text(text):
            Self.textContent(text, replyTo: replyTo)
        case let .richText(text, _):
            text.map { Self.textContent($0, replyTo: replyTo) } ?? .unsupported(nil)
        case let .event(event):
            event.nativeContent
        case .call,
             .payment:
            .unsupported(nil)
        }
    }
}

private extension ChatMessageBody {
    static func textContent(_ text: String, replyTo: String?) -> Chat.LocalMessage.Content {
        guard let replyTo else {
            return .text(text)
        }

        return .reply(.init(
            messageId: replyTo,
            ownContent: ChatRemoteMessageContent.RichText(text: text, attachments: nil)
        ))
    }
}

extension Chat.LocalMessage.Content {
    /// The user action that sends this content to the product, or nil when a product room cannot carry it.
    var productUserAction: ChatUserAction? {
        switch self {
        case let .text(text):
            .newMessage(body: .text(text: text), replyTo: nil, messageId: nil, timestamp: nil)
        case let .reply(reply):
            .newMessage(
                body: .text(text: reply.ownContent.text ?? ""),
                replyTo: reply.messageId,
                messageId: nil,
                timestamp: nil
            )
        case let .edited(edit):
            .editMessage(messageId: edit.messageId, body: .text(text: edit.newContent.text ?? ""))
        case let .reacted(reaction):
            .reactionPlaced(messageId: reaction.messageId, emoji: reaction.emoji)
        case let .reactionRemoved(reaction):
            .reactionRemoved(messageId: reaction.messageId, emoji: reaction.emoji)
        default:
            nil
        }
    }
}

private extension ChatContactEvent {
    var nativeContent: Chat.LocalMessage.Content {
        switch self {
        case .requestAccepted:
            .chatAccepted(.init(messageId: ""))
        case .contactAdded:
            .contactAdded
        case .leftChat:
            .leftChat
        case .unavailable:
            // The native chat shows nothing for messages that it could not retrieve.
            .compactedMessages(.init(claimIdentifier: Data(), claimTicket: Data(), node: .wssUrl("")))
        case .unsupported:
            .unsupported(nil)
        }
    }
}
