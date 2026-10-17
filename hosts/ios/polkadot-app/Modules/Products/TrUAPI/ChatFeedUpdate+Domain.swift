import Foundation
import Products
import TrUAPIHost

// Maps a feed update of the bindings to the values that the chat store writes (P4.1). Each body goes into a
// content kind that the room screen draws already: a text into `.text`, a custom body into
// `.customRendered`, and rich text, files and actions into `.productBody`.

extension Chat.ProductFeedUpdate {
    init(_ update: ChatFeedUpdate) {
        switch update {
        case let .new(message):
            self = .new(Chat.ProductFeedMessage(
                messageId: message.messageId,
                participantId: message.origin.participantId,
                timestamp: message.timestamp,
                content: message.body.localContent,
                replyTo: message.replyTo
            ))
        case let .edited(messageId, body, editedAt):
            self = .edited(messageId: messageId, content: body.localContent, editedAt: editedAt)
        case let .deleted(messageId):
            self = .deleted(messageId: messageId)
        case let .reactionPlaced(messageId, participantId, emoji):
            self = .reactionPlaced(messageId: messageId, participantId: participantId, emoji: emoji)
        case let .reactionRemoved(messageId, participantId, emoji):
            self = .reactionRemoved(messageId: messageId, participantId: participantId, emoji: emoji)
        case let .statusChanged(messageId, status):
            self = .statusChanged(messageId: messageId, status: Chat.ProductMessageStatus(status))
        }
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

extension ChatMessageBody {
    var localContent: Chat.LocalMessage.Content {
        switch self {
        case let .text(text):
            .text(text)
        case let .richText(text, media):
            Self.richTextContent(text: text, media: media)
        case let .actions(actions):
            Self.actionsContent(actions)
        case let .custom(messageType, payload, previewText):
            ProductBotMessage.custom(messageType: messageType, data: payload, alt: previewText).toChatMessageContent()
        case let .notice(text):
            .productBody(.notice(text))
        }
    }

    private static func actionsContent(_ actions: ChatActions) -> Chat.LocalMessage.Content {
        let message = ProductBotMessage.actions(ProductBotActions(
            text: actions.text,
            buttons: actions.actions.map { .init(actionId: $0.actionId, title: $0.title) },
            layout: actions.layout == .grid ? .grid : .column
        ))
        return message.toChatMessageContent()
    }

    /// One file with no other media is a file bubble. Else the images, the videos and the files go into one media
    /// bubble, which draws a file among them as a row (02-DESIGN §6.2, PARITY row 4).
    private static func richTextContent(text: String?, media: [ChatMediaItem]) -> Chat.LocalMessage.Content {
        if media.count == 1, case let .file(_, meta, fileName) = media[0] {
            let message = ProductBotMessage.file(ProductBotFile(
                url: media[0].source,
                fileName: fileName,
                mimeType: meta.mimeType,
                sizeBytes: meta.sizeBytes ?? 0,
                text: text
            ))
            return message.toChatMessageContent()
        }

        return .productBody(.richText(.init(
            text: text,
            media: media.map(\.source),
            alts: media.map(\.alt),
            items: media.map(\.info)
        )))
    }
}

extension ChatMediaItem {
    /// The text alternative of the item, for VoiceOver.
    var alt: String? {
        switch self {
        case let .image(_, meta),
             let .video(_, meta),
             let .file(_, meta, _):
            meta.alt
        }
    }

    /// The source string that a product body keeps for the item (see ``ChatResource/init(source:)``).
    var source: String {
        switch self {
        case let .image(.inline(mimeType, bytes), _),
             let .video(.inline(mimeType, bytes), _),
             let .file(.inline(mimeType, bytes), _, _):
            Chat.ProductResource.inline(mimeType: mimeType, data: bytes).source
        case let .image(.preimage(key), _),
             let .video(.preimage(key), _),
             let .file(.preimage(key), _, _):
            ChatResource.preimageSource(key: key)
        case let .image(.sealed(parts, key, nonce, size, digest), _),
             let .video(.sealed(parts, key, nonce, size, digest), _),
             let .file(.sealed(parts, key, nonce, size, digest), _, _):
            ChatResource.sealedSource(ChatSealedResource(
                parts: parts,
                key: key,
                nonce: nonce,
                size: size,
                digest: digest
            ))
        }
    }

    /// The kind of the item, which the media bubble needs to open it.
    var info: Chat.LocalMessage.Content.ProductMediaInfo {
        switch self {
        case let .image(_, meta):
            .init(kind: .image, mimeType: meta.mimeType, fileName: nil, sizeBytes: meta.sizeBytes)
        case let .video(_, meta):
            .init(kind: .video, mimeType: meta.mimeType, fileName: nil, sizeBytes: meta.sizeBytes)
        case let .file(_, meta, fileName):
            .init(kind: .file, mimeType: meta.mimeType, fileName: fileName, sizeBytes: meta.sizeBytes)
        }
    }
}

extension Chat.ProductResource {
    /// The source string that a product body keeps for a media item: an inline `data:` URL, or a preimage key
    /// that `ChatResource(source:)` reads back.
    var source: String {
        switch self {
        case let .inline(mimeType, data): "data:\(mimeType);base64,\(data.base64EncodedString())"
        case let .preimage(key): ChatResource.preimageSource(key: key)
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
