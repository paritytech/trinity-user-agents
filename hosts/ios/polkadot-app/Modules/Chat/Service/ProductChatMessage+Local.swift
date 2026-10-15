import Foundation
import TrUAPIHost

/// The participant id that names the user of the host in a product room (02-DESIGN §3.3.3).
let productUserParticipantId = "user"

extension ChatMessage {
    /// The message as the room screen draws it.
    ///
    /// - Parameters:
    ///   - chatId: The chat of the room.
    ///   - extensionId: The extension of the product. Messages of other participants come from it.
    ///   - isUnread: True when a message of another participant comes after the last message that the user saw.
    func localMessage(chatId: Chat.Id, extensionId: ChatExtension.Id, isUnread: Bool) -> Chat.LocalMessage {
        let isFromUser = origin.participantId == productUserParticipantId
        let localStatus: Chat.LocalMessage.Status = isFromUser
            ? .outgoing(status.outgoingStatus)
            : .incoming(isUnread ? .new : .seen)

        var message = Chat.LocalMessage(
            messageId: messageId,
            chatId: chatId,
            origin: isFromUser ? .user : .chatExtension(extensionId),
            creationSource: .localDevice,
            status: localStatus,
            timestamp: timestamp,
            content: body.localContent(replyTo: replyTo),
            reactions: localReactions(chatId: chatId, extensionId: extensionId),
            compactionId: nil,
            relatedMessages: []
        )

        if !isFromUser, origin.participantId != Chat.ProductParticipant.product {
            message.authorParticipantId = origin.participantId
        }
        return message
    }

    var isFromUser: Bool {
        origin.participantId == productUserParticipantId
    }

    /// One reaction for each count of a summary. The user reaction has the user origin.
    private func localReactions(chatId: Chat.Id, extensionId: ChatExtension.Id) -> [Chat.MessageReaction] {
        reactions.flatMap { summary in
            let total = Int(summary.count)
            let others = summary.reactedByUser ? max(total - 1, 0) : total
            let userReaction: [Chat.LocalMessage.Origin] = summary.reactedByUser ? [.user] : []
            let productReactions = Array(repeating: Chat.LocalMessage.Origin.chatExtension(extensionId), count: others)
            let origins = userReaction + productReactions

            return origins.map { origin in
                Chat.MessageReaction(
                    messageId: messageId,
                    emoji: summary.emoji,
                    origin: origin,
                    chatId: chatId,
                    timestamp: timestamp
                )
            }
        }
    }
}

extension ChatMessageStatus {
    /// The tick of an outgoing message. `Preparing` draws as a message on its way.
    var outgoingStatus: Chat.LocalMessage.Status.OutgoingStatus {
        switch self {
        case .preparing: .new
        case .sent: .sent
        case .delivered: .delivered
        case .read: .read
        case .failed: .failed
        }
    }
}

extension ChatMessageBody {
    /// The body as a content kind of the room screen.
    ///
    /// A reply quote exists only for a text body. Media that is not inline is not drawn yet (P6.7).
    func localContent(replyTo: String?) -> Chat.LocalMessage.Content {
        switch self {
        case let .text(text):
            if let replyTo {
                .reply(.init(messageId: replyTo, ownContent: .init(text: text, attachments: nil)))
            } else {
                .text(text)
            }
        case let .richText(text, media):
            .productBody(.richText(.init(
                text: text,
                media: media.compactMap(\.inlineDataURL),
                alts: media.filter { $0.inlineDataURL != nil }.map(\.alt)
            )))
        case let .actions(actions):
            .productBody(.actions(.init(
                text: actions.text,
                buttons: actions.actions.map { .init(actionId: $0.actionId, title: $0.title) },
                layout: actions.layout == .grid ? .grid : .column
            )))
        case let .custom(messageType, payload, previewText):
            .customRendered(.init(
                decoderId: MessageDecoderIdentifier.product.rawValue,
                data: payload,
                identifier: messageType,
                alt: previewText
            ))
        }
    }

    /// The text of the body, for an edit or a preview.
    var text: String? {
        switch self {
        case let .text(text): text
        case let .richText(text, _): text
        case let .actions(actions): actions.text
        case let .custom(_, _, previewText): previewText
        }
    }
}

private extension ChatMediaItem {
    /// An image or a video with inline bytes, as a `data:` URL.
    var inlineDataURL: String? {
        switch self {
        case let .image(resource, _),
             let .video(resource, _):
            guard case let .inline(mimeType, bytes) = resource else {
                return nil
            }
            return "data:\(mimeType);base64,\(bytes.base64EncodedString())"
        case .file:
            return nil
        }
    }
}

extension Chat.LocalMessage.Content {
    /// The user action that sends this content to the product, or nil when a product room cannot carry it.
    var productUserAction: ChatUserAction? {
        switch self {
        case let .text(text):
            .newMessage(body: .text(text: text), replyTo: nil)
        case let .reply(reply):
            .newMessage(body: .text(text: reply.ownContent.text ?? ""), replyTo: reply.messageId)
        case let .richText(richText):
            richText.text.map { .newMessage(body: .text(text: $0), replyTo: nil) }
        case let .edited(edit):
            .editMessage(messageId: edit.messageId, body: .text(text: edit.newContent.text ?? ""))
        case let .reacted(reaction):
            .reactionPlaced(messageId: reaction.messageId, emoji: reaction.emoji)
        case let .reactionRemoved(reaction):
            .reactionRemoved(messageId: reaction.messageId, emoji: reaction.emoji)
        case let .productBody(body):
            body.userWireBody.map { .newMessage(body: $0, replyTo: nil) }
        default:
            nil
        }
    }
}
