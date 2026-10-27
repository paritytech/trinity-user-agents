import Foundation
import TrUAPIHost

/// The participant id that names the user of the host in a product room.
let productUserParticipantId = "user"

// A message of a product room. The room screen and the chat list then draw it with the views and the strings of the
// native person chat: a text with Markdown and links, or an info row.

extension ChatMessage {
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
