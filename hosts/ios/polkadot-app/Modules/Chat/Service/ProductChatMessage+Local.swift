import Foundation
import SubstrateSdk
import TrUAPIHost

/// The participant id that names the user of the host in a product room (02-DESIGN §3.3.3).
let productUserParticipantId = "user"

// A message of a product room as the native chat keeps it. The room screen, the chat list and the notifications
// then draw it with the views and the strings of the native person chat: a text with Markdown and links, a media
// grid, a call bubble, a transfer bubble, an info row or the "Request message sent" row.

extension ChatMessage {
    /// The message as the room screen draws it.
    ///
    /// - Parameters:
    ///   - chatId: The chat of the room.
    ///   - extensionId: The extension of the product. Messages of other participants come from it.
    ///   - isSeen: True when the user saw the message on this device and the product has not marked it read yet.
    ///   - transferState: The state of the transfer of a payment, which the host keeps by message id.
    func localMessage(
        chatId: Chat.Id,
        extensionId: ChatExtension.Id,
        isSeen: Bool = false,
        transferState: Chat.LocalMessage.Content.Transfer.State? = nil
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
            content: nativeContent(transferState: transferState),
            reactions: localReactions(chatId: chatId, extensionId: extensionId),
            compactionId: nil,
            relatedMessages: relatedMessages(status: localStatus)
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
            relatedMessages: relatedMessages(status: localStatus) + editRelatedMessages(status: localStatus)
        )
    }

    var isFromUser: Bool {
        origin.participantId == productUserParticipantId
    }

    /// A message of the contact that the user has not seen: the product gives it `Delivered` until the user sees
    /// it and `Read` after, as the native chat keeps an incoming message new until it is seen.
    var isUnreadByUser: Bool {
        !isFromUser && status == .delivered
    }

    /// The content of the native chat. The welcome message of a chat request is the request of the native chat,
    /// which draws the "Request message sent" row above an outgoing one.
    var nativeContent: Chat.LocalMessage.Content {
        nativeContent(transferState: nil)
    }

    /// The content of the native chat, with the state of the transfer of a payment. The native transfer bubble reads
    /// it as it reads the state row of a native message: no state is a payment that the monitor did not follow yet.
    func nativeContent(transferState: Chat.LocalMessage.Content.Transfer.State?) -> Chat.LocalMessage.Content {
        if request, case let .text(text) = body {
            return .versionedChatRequest(.v1(Chat.RequestContentV1(
                pushToken: nil,
                welcomeMessage: ChatRemoteMessageContent.RichText(text: text, attachments: nil)
            )))
        }

        // The coin keys stay with the host, which keeps the payment and its state by message id.
        if case let .payment(payment) = body, !payment.legacy {
            return .coinageSend(.init(totalValue: Balance(payment.amount) ?? 0, coinKeys: [], state: transferState))
        }

        return body.nativeContent(replyTo: replyTo)
    }

    /// The answer and the end of a call, which the native call bubble reads to resolve the state of the call
    /// (`Chat.LocalMessage.resolveCallState()`). The offer time is the time of the message.
    private func relatedMessages(status: Chat.LocalMessage.Status) -> [Chat.RelatedLocalMessage] {
        guard case let .call(record) = body else {
            return []
        }

        var related: [Chat.RelatedLocalMessage] = []

        if let answeredAt = record.answeredAt {
            related.append(Chat.RelatedLocalMessage(
                messageId: messageId + "/answer",
                timestamp: answeredAt,
                content: .call(.answer(.init(offerId: messageId, sdp: Data()))),
                status: status
            ))
        }

        if let closed = record.closed {
            related.append(Chat.RelatedLocalMessage(
                messageId: messageId + "/closed",
                timestamp: closed.at,
                content: .call(.closed(.init(offerId: messageId))),
                status: status
            ))
        }

        return related
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
    /// The tick of an outgoing message, as the native chat draws it. `Preparing` is a message on its way. The
    /// product sends `Read` when a device of the contact took the message, which the native chat draws as
    /// delivered.
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
    /// A reply quote exists for a text body and a media body. A `RichText` body with no media is a text body.
    func nativeContent(replyTo: String?) -> Chat.LocalMessage.Content {
        switch self {
        case let .text(text):
            Self.richContent(text: text, attachments: nil, replyTo: replyTo)
        case let .richText(text, media):
            Self.richContent(text: text, attachments: media.nativeAttachments, replyTo: replyTo)
        case let .event(event):
            event.nativeContent
        case let .call(record):
            .call(.offer(.init(sdp: Data(), purpose: record.video ? .video : .audio)))
        case let .payment(payment):
            payment.nativeContent
        }
    }
}

private extension ChatMessageBody {
    static func richContent(
        text: String?,
        attachments: [ChatRemoteMessageContent.FileVariant]?,
        replyTo: String?
    ) -> Chat.LocalMessage.Content {
        let attachments = attachments?.isEmpty == true ? nil : attachments

        if let replyTo {
            return .reply(.init(
                messageId: replyTo,
                ownContent: ChatRemoteMessageContent.RichText(text: text, attachments: attachments)
            ))
        }

        if let attachments {
            return .richText(.init(text: text, attachments: attachments.map { .remoteDownloadable($0) }))
        }

        return .text(text ?? "")
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

private extension ChatPaymentRecord {
    var nativeContent: Chat.LocalMessage.Content {
        // The core gives the amount as its decimal text.
        let amount = Balance(amount) ?? 0

        guard legacy else {
            return .coinageSend(.init(totalValue: amount, coinKeys: []))
        }

        return .send(.init(amount: amount, blockHash: Data(), extrinsicHash: Data()))
    }
}

extension [ChatMediaItem] {
    /// The photos and the videos, as the native chat keeps the files of a message. The native chat draws no other
    /// file in a media message.
    var nativeAttachments: [ChatRemoteMessageContent.FileVariant] {
        compactMap { item in
            switch item {
            case let .image(.hop(identifier, ticket, node, size), meta):
                .p2pMixnet(.init(
                    identifier: identifier,
                    claimTicket: ticket,
                    node: .wssUrl(node),
                    meta: .image(.init(
                        general: meta.general(size: size),
                        width: meta.width ?? 0,
                        height: meta.height ?? 0,
                        thumbnail: meta.placeholder.map { Data($0.utf8) }
                    ))
                ))
            case let .video(.hop(identifier, ticket, node, size), meta):
                .p2pMixnet(.init(
                    identifier: identifier,
                    claimTicket: ticket,
                    node: .wssUrl(node),
                    meta: .video(.init(
                        general: meta.general(size: size),
                        duration: meta.durationSeconds,
                        thumbnail: meta.placeholder.map { Data($0.utf8) }
                    ))
                ))
            case .file:
                nil
            }
        }
    }
}

private extension ChatMediaMeta {
    func general(size: UInt64) -> ChatRemoteMessageContent.GeneralFileMeta {
        .init(mimeType: mimeType, fileSize: UInt32(clamping: sizeBytes ?? size))
    }

    /// The length of a video in whole seconds, as the native chat keeps it. Zero when the product gave none.
    var durationSeconds: UInt32 {
        UInt32(clamping: (durationMs ?? 0) / 1_000)
    }
}

extension Chat.LocalMessage.Content {
    /// The user action that sends this content to the product, or nil when a product room cannot carry it. A
    /// text, a photo or a video goes through ``ProductOutbox``, which chooses its id.
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
        case let .versionedChatRequest(request):
            request.ensureV1().welcomeMessage?.text.map { .newMessage(
                body: .text(text: $0),
                replyTo: nil,
                messageId: nil,
                timestamp: nil
            ) }
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
