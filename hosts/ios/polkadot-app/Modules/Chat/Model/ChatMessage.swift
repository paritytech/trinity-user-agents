import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

import UIKit.UIImage

extension Chat {
    typealias MessageId = String
    typealias Timestamp = UInt64

    struct LocalMessage: Equatable {
        let messageId: MessageId
        let chatId: Chat.Id
        let origin: Chat.LocalMessage.Origin
        let creationSource: CreationSource
        let status: Status
        let timestamp: Timestamp
        let content: Content

        // fields below are ignored when saved
        // we probably need to represent fetching and
        // saving messages with different models
        let reactions: [Chat.MessageReaction]
        let compactionId: MessageId?
        let relatedMessages: [Chat.RelatedLocalMessage]
        let order: UInt64

        init?(
            remote: Chat.RemoteMessage,
            creationSource: CreationSource,
            status: Status,
            contactId: AccountId
        ) {
            self.init(
                remote: remote,
                creationSource: creationSource,
                status: status,
                chatId: .person(contactId),
                origin: .contact(contactId)
            )
        }

        init?(
            remote: Chat.RemoteMessage,
            creationSource: CreationSource,
            status: Status,
            chatId: Chat.Id,
            origin: Chat.LocalMessage.Origin
        ) {
            guard let content = Content(remote: remote.versioned) else {
                return nil
            }

            messageId = remote.messageId
            self.chatId = chatId
            self.origin = origin
            self.creationSource = creationSource
            self.status = status
            timestamp = remote.timestamp
            self.content = content
            reactions = []
            compactionId = nil
            relatedMessages = []
            order = 0
        }

        init(
            chatRequest: Chat.RequestMessage,
            creationSource: CreationSource,
            status: Status,
            contactId: AccountId
        ) {
            messageId = chatRequest.messageId
            chatId = Chat.Id.person(contactId)
            origin = .contact(contactId)
            self.creationSource = creationSource
            self.status = status
            timestamp = chatRequest.timestamp
            content = .versionedChatRequest(chatRequest.content)
            reactions = []
            compactionId = nil
            relatedMessages = []
            order = 0
        }

        init(
            messageId: MessageId,
            chatId: Chat.Id,
            origin: Origin,
            creationSource: CreationSource,
            status: Status,
            timestamp: Timestamp,
            content: Content,
            reactions: [Chat.MessageReaction],
            compactionId: MessageId?,
            relatedMessages: [Chat.RelatedLocalMessage],
            order: UInt64 = 0
        ) {
            self.messageId = messageId
            self.chatId = chatId
            self.origin = origin
            self.creationSource = creationSource
            self.status = status
            self.timestamp = timestamp
            self.content = content
            self.reactions = reactions
            self.compactionId = compactionId
            self.relatedMessages = relatedMessages
            self.order = order
        }
    }

    /// Lightweight version of a LocalMessage
    /// Never add `relatedMessages` that defeats the purpose.
    struct RelatedLocalMessage: Equatable {
        let messageId: MessageId
        let timestamp: Timestamp
        let content: Chat.LocalMessage.Content
        let status: Chat.LocalMessage.Status
    }
}

extension Chat.LocalMessage {
    var asRelated: Chat.RelatedLocalMessage {
        Chat.RelatedLocalMessage(
            messageId: messageId,
            timestamp: timestamp,
            content: content,
            status: status
        )
    }
}

extension Chat.LocalMessage {
    enum CreationSource: Int16, Equatable {
        case localDevice
        case deviceSync
    }
}

extension Chat.LocalMessage {
    enum Origin: Equatable {
        case user
        case contact(AccountId)
        case chatExtension(String)
    }
}

extension Chat.LocalMessage {
    enum Content: Equatable {
        case text(String)
        case token(Chat.RemoteTokenContent)
        case send(Chat.RemoteMessageContentV1.MessageContent.SendContent.Legacy)
        case contactAdded
        case leftChat
        case reacted(Chat.RemoteMessageContentV1.MessageContent.ReactionContent)
        case reactionRemoved(Chat.RemoteMessageContentV1.MessageContent.ReactionContent)
        case edited(Chat.RemoteMessageContentV1.MessageContent.EditedContent)
        case reply(Chat.RemoteMessageContentV1.MessageContent.ReplyContent)
        case chatAccepted(Chat.RemoteMessageContentV1.MessageContent.ChatAccepted)
        case unsupported(Data?)
        case staticTextImageContent(StaticTextImageContent) // TODO: migrate to custom renderer
        case extensionActionResponse(
            String,
            String
        ) // TODO: shouldn't be content type but rather handled via extension actions method
        case customRendered(CustomRenderedData)
        // TODO: remove chatRequest case after data wipe
        case chatRequest(Chat.RequestContentV1)
        case versionedChatRequest(Chat.VersionedRequestContent)
        case file(File)
        case richText(RichText)
        case coinageSend(Transfer)
        case compactedMessages(ChatRemoteMessageContent.CompactedMessagesContent)
        case deviceAdded(Chat.RemoteMessageContentV1.MessageContent.DeviceAddedContent)
        case deviceRemoved(Chat.RemoteMessageContentV1.MessageContent.DeviceRemovedContent)
        case multiChatAccepted(Chat.RemoteMessageContentV1.MessageContent.DeviceChatAccepted)
        case call(CallSignalingPayload)

        enum CallSignalingPayload: Equatable {
            case offer(Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent)
            case answer(Chat.RemoteMessageContentV1.MessageContent.DataChannelAnswerContent)
            // TODO: We don't save candidates localy anymore. Remove on wipe
            case candidates(Chat.RemoteMessageContentV1.MessageContent.DataChannelCandidatesContent)
            case closed(Chat.RemoteMessageContentV1.MessageContent.DataChannelClosedContent)
        }

        // swiftlint:disable:next cyclomatic_complexity
        init?(remoteContent: Chat.RemoteMessageContentV1) {
            switch remoteContent.content {
            case let .text(content):
                self = .text(content)
            case let .token(content):
                self = .token(content)
            case let .send(content):
                self = .send(content)
            case .contactAdded:
                self = .contactAdded
            case .leftChat:
                self = .leftChat
            case let .reacted(content):
                self = .reacted(content)
            case let .reactionRemoved(content):
                self = .reactionRemoved(content)
            case let .edited(content):
                self = .edited(content)
            case let .chatAccepted(content):
                self = .chatAccepted(content)
            case let .compactedMessages(content):
                self = .compactedMessages(content)
            case let .dataChannelOffer(content):
                self = .call(.offer(content))
            case let .dataChannelAnswer(content):
                self = .call(.answer(content))
            case .dataChannelCandidates:
                return nil
            case let .dataChannelClosed(content):
                self = .call(.closed(content))
            case let .reply(content):
                self = .reply(content)
            case let .richText(content):
                let richText = RichText(remoteRichText: content)
                self = .richText(richText)
            case let .coinageSend(content):
                self = .coinageSend(Transfer(content))
            case let .deviceAdded(content):
                self = .deviceAdded(content)
            case let .deviceRemoved(content):
                self = .deviceRemoved(content)
            case let .multiChatAccepted(content):
                self = .multiChatAccepted(content)
            }
        }

        init?(remote: Chat.VersionedRemoteMessageContent) {
            switch remote {
            case let .v1(remoteContent):
                self.init(remoteContent: remoteContent)
            case let .unsupported(data):
                self = .unsupported(data)
            }
        }
    }
}

extension Chat.LocalMessage.Content {
    //    text(TextContent) -> 0
    //    token(TokenContent) -> 1
    //    send(SendContent) -> 2
    //    contactAdded -> 3
    //    reacted(ReactionContent) -> 4
    //    reactionRemoved(ReactionContent) -> 5
    //    reply(ReplyContent) -> 7
    //    edited(EditedContent) -> 12
    //    leftChat -> 13
    //    chatAccepted -> 14
    //    richText -> 15
    //    coinagePayment(CoinagePaymentContent) -> 16
    // DON'T change the indexes to be compatible with db
    enum ContentType: UInt8 {
        case text = 0
        case token = 1
        case send = 2
        case contactAdded = 3
        case reacted = 4
        case reactionRemoved = 5
        case reply = 7
        case edited = 12
        case leftChat = 13
        case chatAccepted = 14
        case richText = 15
        case coinageSend = 16
        case deviceAdded = 17
        case deviceRemoved = 18
        case compactedMessages = 19
        case multiChatAccepted = 20
        case versionedChatRequest = 248
        case call = 249
        case unsupported = 255
        case staticImageText = 254
        case extensionActionResponse = 253
        case customRendered = 252
        case chatRequest = 251
        case file = 250

        var isIndependentMessageInChat: Bool {
            !isSystem && !isReaction && !isEdit
        }

        // Used by chat list to skip
        // messages that shouldn't appear as the chat preview. Kept separate from
        // `isIndependentMessageInChat`, which serves layout/grouping in the
        // message feed — so changing one won't silently affect other
        var isExcludedFromChatList: Bool {
            isSystem || isReaction || isEdit
        }

        var isGroupable: Bool {
            switch self {
            case .text,
                 .reply,
                 .richText:
                true
            default:
                false
            }
        }

        var isSystem: Bool {
            switch self {
            case .text,
                 .send,
                 .coinageSend,
                 .contactAdded,
                 .leftChat,
                 .reply,
                 .unsupported,
                 .staticImageText,
                 .reacted,
                 .reactionRemoved,
                 .edited,
                 .chatAccepted,
                 .multiChatAccepted,
                 .extensionActionResponse,
                 .chatRequest,
                 .versionedChatRequest,
                 .customRendered,
                 .file,
                 .richText,
                 .call:
                false
            case .token,
                 .compactedMessages,
                 .deviceAdded,
                 .deviceRemoved:
                true
            }
        }

        var isReaction: Bool {
            switch self {
            case .reacted,
                 .reactionRemoved:
                true
            case .text,
                 .send,
                 .coinageSend,
                 .contactAdded,
                 .leftChat,
                 .reply,
                 .unsupported,
                 .staticImageText,
                 .token,
                 .edited,
                 .chatAccepted,
                 .multiChatAccepted,
                 .extensionActionResponse,
                 .chatRequest,
                 .versionedChatRequest,
                 .customRendered,
                 .file,
                 .richText,
                 .compactedMessages,
                 .deviceAdded,
                 .deviceRemoved,
                 .call:
                false
            }
        }

        var isEdit: Bool {
            switch self {
            case .edited:
                true
            case .text,
                 .send,
                 .coinageSend,
                 .contactAdded,
                 .reply,
                 .unsupported,
                 .staticImageText,
                 .token,
                 .reacted,
                 .leftChat,
                 .reactionRemoved,
                 .chatAccepted,
                 .multiChatAccepted,
                 .extensionActionResponse,
                 .chatRequest,
                 .versionedChatRequest,
                 .customRendered,
                 .file,
                 .richText,
                 .compactedMessages,
                 .deviceAdded,
                 .deviceRemoved,
                 .call:
                false
            }
        }
    }

    var contentType: Chat.LocalMessage.Content.ContentType {
        switch self {
        case .contactAdded:
            .contactAdded
        case .leftChat:
            .leftChat
        case .text:
            .text
        case .token:
            .token
        case .send:
            .send
        case .reacted:
            .reacted
        case .reactionRemoved:
            .reactionRemoved
        case .edited:
            .edited
        case .reply:
            .reply
        case .chatAccepted:
            .chatAccepted
        case .unsupported:
            .unsupported
        case .staticTextImageContent:
            .staticImageText
        case .extensionActionResponse:
            .extensionActionResponse
        case .chatRequest:
            .chatRequest
        case .versionedChatRequest:
            .versionedChatRequest
        case .customRendered:
            .customRendered
        case .file:
            .file
        case .richText:
            .richText
        case .coinageSend:
            .coinageSend
        case .compactedMessages:
            .compactedMessages
        case .deviceAdded:
            .deviceAdded
        case .deviceRemoved:
            .deviceRemoved
        case .multiChatAccepted:
            .multiChatAccepted
        case .call:
            .call
        }
    }

    var contentKey: String? {
        switch self {
        case let .edited(content):
            content.messageId
        case let .chatAccepted(content):
            // we don't use it for now for filtering
            // but still good to have for future use cases
            content.messageId
        case let .multiChatAccepted(content):
            // we don't use it for now for filtering
            // but still good to have for future use cases
            content.requestId
        case let .customRendered(data):
            data.identifier
        case .text,
             .token,
             .send,
             .coinageSend,
             .contactAdded,
             .leftChat,
             .reacted,
             .reactionRemoved,
             .reply,
             .unsupported,
             .staticTextImageContent,
             .extensionActionResponse,
             .chatRequest,
             .versionedChatRequest,
             .file,
             .richText,
             .compactedMessages,
             .deviceAdded,
             .deviceRemoved,
             .call:
            nil
        }
    }

    var originalMessageText: String? {
        switch self {
        case let .text(text): text
        case let .reply(replyContent): replyContent.ownContent.text
        case let .staticTextImageContent(textImage): textImage.text
        case let .richText(richText): richText.text
        default: nil
        }
    }
}

extension Chat.LocalMessage {
    var isIndependentMessageInChat: Bool {
        content.contentType.isIndependentMessageInChat
    }

    var isExcludedFromChatList: Bool {
        content.contentType.isExcludedFromChatList
    }

    var isGroupable: Bool {
        content.contentType.isGroupable
    }

    var isSystem: Bool {
        content.contentType.isSystem
    }

    var isReaction: Bool {
        content.contentType.isReaction
    }

    var isTransfer: Bool {
        content.contentType == .send || content.contentType == .coinageSend
    }

    // If groupingId is specified, it will be used to group messages in ChatMessageEntityMapper
    var groupingId: String {
        switch content {
        case let .call(payload):
            switch payload {
            case .offer:
                messageId
            case let .answer(content):
                content.offerId
            case let .candidates(content):
                content.offerId
            case let .closed(content):
                content.offerId
            }
        case let .edited(content):
            content.messageId
        default:
            messageId
        }
    }

    func replacingStatus(_ newStatus: Status) -> Self {
        Chat.LocalMessage(
            messageId: messageId,
            chatId: chatId,
            origin: origin,
            creationSource: creationSource,
            status: newStatus,
            timestamp: timestamp,
            content: content,
            reactions: reactions,
            compactionId: compactionId,
            relatedMessages: relatedMessages,
            order: order
        )
    }

    func replacingContent(_ newContent: Chat.LocalMessage.Content) -> Self {
        Chat.LocalMessage(
            messageId: messageId,
            chatId: chatId,
            origin: origin,
            creationSource: creationSource,
            status: status,
            timestamp: timestamp,
            content: newContent,
            reactions: reactions,
            compactionId: compactionId,
            relatedMessages: relatedMessages,
            order: order
        )
    }
}

extension Chat.LocalMessage: Operation_iOS.Identifiable {
    var identifier: String { messageId }
}

extension Chat.LocalMessage {
    enum Status: Equatable {
        case incoming(IncomingStatus)
        case outgoing(OutgoingStatus)

        enum OutgoingStatus: String, Equatable {
            case new
            case sent
            case delivered
        }

        enum IncomingStatus: String, Equatable {
            case new
            case seen
        }

        var statusClass: StatusClass {
            switch self {
            case .incoming: .incoming
            case .outgoing: .outgoing
            }
        }

        var isIncoming: Bool {
            statusClass == .incoming
        }

        var isOutgoing: Bool {
            statusClass == .outgoing
        }

        func ensureIncomingStatus() -> IncomingStatus? {
            switch self {
            case let .incoming(incomingStatus):
                incomingStatus
            case .outgoing:
                nil
            }
        }
    }

    enum StatusClass: Equatable {
        case incoming
        case outgoing
    }
}

extension Chat.LocalMessage.Status: RawRepresentable {
    typealias RawValue = Int16

    init?(rawValue: Int16) {
        switch rawValue {
        case 0: self = .incoming(.new)
        case 1: self = .outgoing(.new)
        case 2: self = .outgoing(.sent)
        case 3: self = .outgoing(.delivered)
        case 4: self = .incoming(.seen)
        default: return nil
        }
    }

    var rawValue: Int16 {
        switch self {
        case .incoming(.new): 0
        case .outgoing(.new): 1
        case .outgoing(.sent): 2
        case .outgoing(.delivered): 3
        case .incoming(.seen): 4
        }
    }
}

extension Chat.LocalMessage.Status.IncomingStatus: Comparable {
    static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.progressRank < rhs.progressRank
    }

    private var progressRank: Int {
        switch self {
        case .new: 0
        case .seen: 1
        }
    }
}

extension Chat.LocalMessage.Status.OutgoingStatus: Comparable {
    static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.progressRank < rhs.progressRank
    }

    private var progressRank: Int {
        switch self {
        case .new: 0
        case .sent: 1
        case .delivered: 2
        }
    }
}

extension Chat.LocalMessage {
    var contactAccountId: AccountId? {
        switch chatId {
        case let .person(accountId):
            accountId
        case .chatExtension:
            nil
        }
    }
}

// MARK: -

extension Chat.LocalMessage.Content {
    struct StaticTextImageContent: Equatable {
        let text: String?
        let media: UIImage

        static func == (lhs: StaticTextImageContent, rhs: StaticTextImageContent) -> Bool {
            lhs.text == rhs.text && lhs.media.isEqual(rhs.media)
        }
    }

    struct CustomRenderedData: Equatable, ScaleCodable {
        let decoderId: UInt8
        let data: Data
        let identifier: String
        /// The sender's one-line description, previewed in place of the drawn card.
        let alt: String?

        init(
            decoderId: UInt8,
            data: Data,
            identifier: String,
            alt: String? = nil
        ) {
            self.decoderId = decoderId
            self.data = data
            self.identifier = identifier
            self.alt = alt
        }

        init(scaleDecoder: any ScaleDecoding) throws {
            identifier = try String(scaleDecoder: scaleDecoder)
            decoderId = try UInt8(scaleDecoder: scaleDecoder)
            data = try Data(scaleDecoder: scaleDecoder)
            // Messages stored before `alt` existed end after `data`.
            alt = scaleDecoder.remained > 0 ? try ScaleOption<String>(scaleDecoder: scaleDecoder).value : nil
        }

        func encode(scaleEncoder: any ScaleEncoding) throws {
            try identifier.encode(scaleEncoder: scaleEncoder)
            try decoderId.encode(scaleEncoder: scaleEncoder)
            try data.encode(scaleEncoder: scaleEncoder)
            try ScaleOption(value: alt).encode(scaleEncoder: scaleEncoder)
        }
    }

    struct Transfer: Equatable {
        /// Read from the state row related to the message; never encoded, `nil` until the monitor writes.
        enum State: Equatable {
            case incoming(IncomingTransferState)
            case outgoing(OutgoingTransferState)
        }

        let totalValue: Balance
        let coinKeys: [Data]
        let state: State?

        init(_ remote: Chat.RemoteMessageContentV1.MessageContent.SendContent.Coinage) {
            totalValue = remote.totalValue
            coinKeys = remote.coinKeys
            state = nil
        }

        init(totalValue: Balance, coinKeys: [Data], state: State? = nil) {
            self.totalValue = totalValue
            self.coinKeys = coinKeys
            self.state = state
        }
    }
}
