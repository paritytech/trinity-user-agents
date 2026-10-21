import Foundation
import TrUAPIHost

extension Chat.LocalMessage {
    /// The message as `message_actions_query` carries it to the product (02-DESIGN §13 item 14).
    ///
    /// The ids are the ids of the product. A `Custom` body has an empty payload. Media items are not sent: the
    /// product knows its messages by id, and inline media can be up to 1 MiB each.
    func productQueryMessage(in chatId: Chat.Id) -> ChatMessage {
        let (queryBody, replyTo) = content.productQueryBody(in: chatId)
        let body: ChatMessageBody =
            if hasMarkdownText, case let .text(text) = queryBody {
                .richText(text: text, media: [])
            } else {
                queryBody
            }

        return ChatMessage(
            messageId: Chat.ProductMessageId.product(messageId, in: chatId),
            origin: ChatMessageOrigin(participantId: productQueryParticipantId),
            timestamp: timestamp,
            body: body,
            status: status.productQueryStatus,
            replyTo: replyTo,
            editedAt: nil,
            reactions: productQueryReactions
        )
    }

    /// The participant id of the author: `"user"` for the user, else the participant that the product named.
    var productQueryParticipantId: String {
        switch origin {
        case .user: Chat.ProductParticipant.user
        case .contact,
             .chatExtension: authorParticipantId ?? Chat.ProductParticipant.product
        }
    }

    private var productQueryReactions: [ChatReactionSummary] {
        var order: [String] = []
        var counts: [String: (count: UInt32, byUser: Bool)] = [:]

        for reaction in reactions {
            if counts[reaction.emoji] == nil {
                order.append(reaction.emoji)
            }
            let current = counts[reaction.emoji] ?? (0, false)
            counts[reaction.emoji] = (current.count + 1, current.byUser || reaction.origin == .user)
        }

        return order.compactMap { emoji in
            counts[emoji].map { ChatReactionSummary(emoji: emoji, count: $0.count, reactedByUser: $0.byUser) }
        }
    }
}

private extension Chat.LocalMessage.Status {
    var productQueryStatus: ChatMessageStatus {
        switch self {
        case .incoming: .sent
        case .outgoing(.new): .preparing
        case .outgoing(.sent): .sent
        case .outgoing(.delivered): .delivered
        case .outgoing(.read): .read
        case .outgoing(.failed): .failed
        }
    }
}

private extension Chat.LocalMessage.Content {
    func productQueryBody(in chatId: Chat.Id) -> (ChatMessageBody, replyTo: String?) {
        switch self {
        case let .text(text),
             let .extensionActionResponse(text, _):
            (.text(text: text), nil)
        case let .reply(reply):
            (
                .text(text: reply.ownContent.text ?? ""),
                Chat.ProductMessageId.product(reply.messageId, in: chatId)
            )
        case let .richText(richText):
            (.richText(text: richText.text, media: []), nil)
        case let .productBody(body):
            (body.productQueryBody, nil)
        case let .customRendered(content):
            (.custom(messageType: content.identifier, payload: Data(), previewText: content.alt), nil)
        default:
            (.text(text: ""), nil)
        }
    }
}

private extension Chat.LocalMessage.Content.ProductBody {
    var productQueryBody: ChatMessageBody {
        switch self {
        case let .richText(richText):
            .richText(text: richText.text, media: [])
        case let .actions(actions):
            .actions(ChatActions(
                text: actions.text,
                actions: actions.buttons.map { ChatAction(actionId: $0.actionId, title: $0.title) },
                layout: actions.layout == .grid ? .grid : .column
            ))
        case let .file(file):
            .richText(text: file.text, media: [])
        case let .notice(text):
            .notice(text: text)
        }
    }
}
