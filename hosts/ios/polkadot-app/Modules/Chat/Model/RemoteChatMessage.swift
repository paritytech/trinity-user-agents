import Foundation
import SubstrateSdk

extension Chat {
    enum RemoteCodingError: Error {
        case unsupportedEncoding
    }

    struct OpaqueMessage: Equatable {
        let remoteMessage: RemoteMessage
    }

    struct RemoteMessage: Equatable {
        let messageId: MessageId // UUID as string
        let timestamp: UInt64
        let versioned: VersionedRemoteMessageContent
    }

    enum VersionedRemoteMessageContent: Equatable {
        // swiftlint:disable:next identifier_name
        case v1(RemoteMessageContentV1)
        case unsupported(Data)

        func ensureV1() -> RemoteMessageContentV1? {
            switch self {
            case let .v1(content):
                content
            case .unsupported:
                nil
            }
        }
    }

    struct RemoteMessageContentV1: Equatable {
        let content: RemoteMessageContentV1.MessageContent
    }
}

extension Chat.RemoteMessage {
    func supportsNotification() -> Bool {
        switch versioned.ensureV1()?.content {
        case .text,
             .send,
             .contactAdded,
             .leftChat,
             .reply,
             .reacted,
             .edited,
             .chatAccepted,
             .multiChatAccepted,
             .richText,
             .dataChannelOffer,
             .coinageSend:
            true
        case .token,
             .dataChannelAnswer,
             .dataChannelCandidates,
             .dataChannelClosed,
             .reactionRemoved,
             .compactedMessages,
             .deviceAdded,
             .deviceRemoved,
             .none:
            false
        }
    }

    func isVoIPNotification() -> Bool {
        switch versioned.ensureV1()?.content {
        case .dataChannelOffer:
            true
        case .token,
             .dataChannelAnswer,
             .dataChannelCandidates,
             .dataChannelClosed,
             .text,
             .send,
             .contactAdded,
             .leftChat,
             .reply,
             .reacted,
             .reactionRemoved,
             .edited,
             .chatAccepted,
             .multiChatAccepted,
             .richText,
             .coinageSend,
             .compactedMessages,
             .deviceAdded,
             .deviceRemoved,
             .none:
            false
        }
    }

    var isCompactedMessages: Bool {
        switch versioned.ensureV1()?.content {
        case .compactedMessages:
            true
        default:
            false
        }
    }
}

extension [Chat.RemoteMessage] {
    func compactionMessageIds() -> Set<String> {
        Set(
            filter(\.isCompactedMessages)
                .map(\.messageId)
        )
    }
}

extension Chat.RemoteMessageContentV1 {
    enum MessageContent: Equatable {
        case text(String)
        case token(Chat.RemoteTokenContent)
        case send(SendContent.Legacy)
        case contactAdded
        case reacted(ReactionContent)
        case reactionRemoved(ReactionContent)
        case reply(ReplyContent)
        case dataChannelOffer(DataChannelOfferContent)
        case dataChannelAnswer(DataChannelAnswerContent)
        case dataChannelCandidates(DataChannelCandidatesContent)
        case dataChannelClosed(DataChannelClosedContent)
        case edited(EditedContent)
        case leftChat
        case chatAccepted(ChatAccepted)
        case richText(RichText)
        case coinageSend(SendContent.Coinage)
        case compactedMessages(CompactedMessagesContent)
        case deviceAdded(DeviceAddedContent)
        case deviceRemoved(DeviceRemovedContent)
        case multiChatAccepted(DeviceChatAccepted)
    }
}

typealias ChatRemoteMessageContent = Chat.RemoteMessageContentV1.MessageContent

extension Chat.RemoteMessageContentV1.MessageContent {
    enum SendContent {
        struct Legacy: Equatable {
            let amount: Balance
            let blockHash: Data
            let extrinsicHash: Data
        }

        struct Coinage: Equatable {
            let totalValue: Balance
            let coinKeys: [Data]
        }

        case legacy(Legacy)
        case coinage(Coinage)

        var amount: Balance {
            switch self {
            case let .legacy(content):
                content.amount
            case let .coinage(content):
                content.totalValue
            }
        }
    }

    enum DataChannelPurpose: UInt8, Equatable {
        case audio = 0
        case video = 1
    }

    struct DataChannelOfferContent: Equatable {
        let sdp: Data
        let purpose: DataChannelPurpose
    }

    struct DataChannelAnswerContent: Equatable {
        let offerId: String
        let sdp: Data
    }

    struct DataChannelCandidatesContent: Equatable {
        let offerId: String
        let sdp: Data
    }

    struct DataChannelClosedContent: Equatable {
        let offerId: String
    }

    struct ReplyContent: Equatable {
        let messageId: String
        let ownContent: RichText
    }

    struct ReactionContent: Equatable {
        let messageId: String
        let emoji: String
    }

    struct EditedContent: Equatable {
        let messageId: String
        let newContent: RichText
    }

    struct ChatAccepted: Equatable {
        let messageId: String
    }

    struct CompactedMessagesContent: Equatable {
        let claimIdentifier: Data
        let claimTicket: Data
        let node: NodeEndpoint
    }

    struct DeviceAddedContent: Equatable {
        let statementAccountId: Data
        let encryptionPublicKey: Data
    }

    struct DeviceRemovedContent: Equatable {
        let statementAccountId: Data
    }

    struct DeviceChatAccepted: Equatable {
        let requestId: String
        let device: Chat.PeerDevice
    }
}

// MARK: - Protocol Conformance

extension Chat.OpaqueMessage: ScaleCodable {
    // encode/decode each message as opaque bytes
    init(scaleDecoder: any ScaleDecoding) throws {
        let bytes = try Data(scaleDecoder: scaleDecoder)

        let bytesDecoder = try ScaleDecoder(data: bytes)

        let messageId = try Chat.MessageId(scaleDecoder: bytesDecoder)
        let timestamp = try UInt64(scaleDecoder: bytesDecoder)

        let remained = bytesDecoder.remained
        let remainedData = try bytesDecoder.read(count: remained)

        do {
            let versioned = try Chat.VersionedRemoteMessageContent(scaleDecoder: bytesDecoder)

            remoteMessage = .init(
                messageId: messageId,
                timestamp: timestamp,
                versioned: versioned
            )
        } catch {
            remoteMessage = .init(
                messageId: messageId,
                timestamp: timestamp,
                versioned: .unsupported(remainedData)
            )
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        let bytesEncoder = ScaleEncoder()

        try remoteMessage.encode(scaleEncoder: bytesEncoder)

        let bytes = bytesEncoder.encode()

        try bytes.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try Chat.MessageId(scaleDecoder: scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        versioned = try Chat.VersionedRemoteMessageContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try versioned.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.VersionedRemoteMessageContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .v1(Chat.RemoteMessageContentV1(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .v1(content):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case .unsupported:
            throw Chat.RemoteCodingError.unsupportedEncoding
        }
    }
}

extension Chat.RemoteMessageContentV1: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        // there might be some content that should be treated as unsupported
        content = try Chat.RemoteMessageContentV1.MessageContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try content.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .text: 0
        case .token: 1
        case .send: 2
        case .contactAdded: 3
        case .reacted: 4
        case .reactionRemoved: 5
        case .reply: 7
        case .dataChannelOffer: 8
        case .dataChannelAnswer: 9
        case .dataChannelCandidates: 10
        case .dataChannelClosed: 11
        case .edited: 12
        case .leftChat: 13
        case .chatAccepted: 14
        case .richText: 15
        case .coinageSend: 16
        case .deviceAdded: 17
        case .deviceRemoved: 18
        case .compactedMessages: 19
        case .multiChatAccepted: 20
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch idx {
        case 0:
            self = try .text(String(scaleDecoder: scaleDecoder))
        case 1:
            self = try .token(Chat.RemoteTokenContent(scaleDecoder: scaleDecoder))
        case 2:
            self = try .send(SendContent.Legacy(scaleDecoder: scaleDecoder))
        case 3:
            self = .contactAdded
        case 4:
            self = try .reacted(ReactionContent(scaleDecoder: scaleDecoder))
        case 5:
            self = try .reactionRemoved(ReactionContent(scaleDecoder: scaleDecoder))
        case 7:
            self = try .reply(ReplyContent(scaleDecoder: scaleDecoder))
        case 8:
            self = try .dataChannelOffer(DataChannelOfferContent(scaleDecoder: scaleDecoder))
        case 9:
            self = try .dataChannelAnswer(DataChannelAnswerContent(scaleDecoder: scaleDecoder))
        case 10:
            self = try .dataChannelCandidates(DataChannelCandidatesContent(scaleDecoder: scaleDecoder))
        case 11:
            self = try .dataChannelClosed(DataChannelClosedContent(scaleDecoder: scaleDecoder))
        case 12:
            self = try .edited(EditedContent(scaleDecoder: scaleDecoder))
        case 13:
            self = .leftChat
        case 14:
            self = try .chatAccepted(ChatAccepted(scaleDecoder: scaleDecoder))
        case 15:
            self = try .richText(ChatRemoteMessageContent.RichText(scaleDecoder: scaleDecoder))
        case 16:
            self = try .coinageSend(SendContent.Coinage(scaleDecoder: scaleDecoder))
        case 17:
            self = try .deviceAdded(DeviceAddedContent(scaleDecoder: scaleDecoder))
        case 18:
            self = try .deviceRemoved(DeviceRemovedContent(scaleDecoder: scaleDecoder))
        case 19:
            self = try .compactedMessages(CompactedMessagesContent(scaleDecoder: scaleDecoder))
        case 20:
            self = try .multiChatAccepted(DeviceChatAccepted(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .text(txt):
            try txt.encode(scaleEncoder: scaleEncoder)
        case let .token(tokenContent):
            try tokenContent.encode(scaleEncoder: scaleEncoder)
        case let .send(sendContent):
            try sendContent.encode(scaleEncoder: scaleEncoder)
        case .contactAdded:
            break
        case .leftChat:
            break
        case let .reacted(reactionContent):
            try reactionContent.encode(scaleEncoder: scaleEncoder)
        case let .reactionRemoved(reactionContent):
            try reactionContent.encode(scaleEncoder: scaleEncoder)
        case let .reply(replyContent):
            try replyContent.encode(scaleEncoder: scaleEncoder)
        case let .dataChannelOffer(offerContent):
            try offerContent.encode(scaleEncoder: scaleEncoder)
        case let .dataChannelAnswer(answerContent):
            try answerContent.encode(scaleEncoder: scaleEncoder)
        case let .dataChannelCandidates(candidatesContent):
            try candidatesContent.encode(scaleEncoder: scaleEncoder)
        case let .dataChannelClosed(closedContent):
            try closedContent.encode(scaleEncoder: scaleEncoder)
        case let .edited(editedContent):
            try editedContent.encode(scaleEncoder: scaleEncoder)
        case let .chatAccepted(chatAcceptedContent):
            try chatAcceptedContent.encode(scaleEncoder: scaleEncoder)
        case let .richText(richTextContent):
            try richTextContent.encode(scaleEncoder: scaleEncoder)
        case let .coinageSend(coinageSendContent):
            try coinageSendContent.encode(scaleEncoder: scaleEncoder)
        case let .compactedMessages(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .deviceAdded(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .deviceRemoved(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .multiChatAccepted(content):
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.CompactedMessagesContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        claimIdentifier = try Data(scaleDecoder: scaleDecoder)
        claimTicket = try Data(scaleDecoder: scaleDecoder)
        node = try Chat.RemoteMessageContentV1.MessageContent.NodeEndpoint(
            scaleDecoder: scaleDecoder
        )
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try claimIdentifier.encode(scaleEncoder: scaleEncoder)
        try claimTicket.encode(scaleEncoder: scaleEncoder)
        try node.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.SendContent.Legacy: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        amount = try Balance(scaleDecoder: scaleDecoder)
        blockHash = try scaleDecoder.readAndConfirm(count: 32)
        extrinsicHash = try scaleDecoder.readAndConfirm(count: 32)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try amount.encode(scaleEncoder: scaleEncoder)
        scaleEncoder.appendRaw(data: blockHash)
        scaleEncoder.appendRaw(data: extrinsicHash)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DataChannelPurpose: ScaleCodable {
    init(scaleDecoder: ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = value
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        sdp = try Data(scaleDecoder: scaleDecoder)
        purpose = try Chat.RemoteMessageContentV1.MessageContent.DataChannelPurpose(
            scaleDecoder: scaleDecoder
        )
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try sdp.encode(scaleEncoder: scaleEncoder)
        try purpose.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DataChannelAnswerContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        offerId = try String(scaleDecoder: scaleDecoder)
        sdp = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try offerId.encode(scaleEncoder: scaleEncoder)
        try sdp.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DataChannelCandidatesContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        offerId = try String(scaleDecoder: scaleDecoder)
        sdp = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try offerId.encode(scaleEncoder: scaleEncoder)
        try sdp.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DataChannelClosedContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        offerId = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try offerId.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.ReplyContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        ownContent = try ChatRemoteMessageContent.RichText(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try ownContent.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.ReactionContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        emoji = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try emoji.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.EditedContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        newContent = try ChatRemoteMessageContent.RichText(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try newContent.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.ChatAccepted: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DeviceAddedContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        statementAccountId = try Data(scaleDecoder: scaleDecoder)
        encryptionPublicKey = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try statementAccountId.encode(scaleEncoder: scaleEncoder)
        try encryptionPublicKey.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DeviceRemovedContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        statementAccountId = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try statementAccountId.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.SendContent.Coinage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        totalValue = try Balance(scaleDecoder: scaleDecoder)
        coinKeys = try [Data](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try totalValue.encode(scaleEncoder: scaleEncoder)
        try coinKeys.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.PeerDevice: ScaleCodable {
    static let accountIdLength = 32
    static let encryptionKeyLength = 32

    init(scaleDecoder: any ScaleDecoding) throws {
        statementAccountId = try scaleDecoder.readAndConfirm(count: Self.accountIdLength)
        encryptionPublicKey = try scaleDecoder.readAndConfirm(count: Self.encryptionKeyLength)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: statementAccountId)
        scaleEncoder.appendRaw(data: encryptionPublicKey)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.DeviceChatAccepted: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        requestId = try String(scaleDecoder: scaleDecoder)
        device = try Chat.PeerDevice(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try requestId.encode(scaleEncoder: scaleEncoder)
        try device.encode(scaleEncoder: scaleEncoder)
    }
}
