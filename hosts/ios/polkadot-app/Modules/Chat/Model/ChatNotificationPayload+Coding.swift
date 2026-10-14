import Foundation
import SubstrateSdk

extension Chat.NotificationPayload: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try Chat.MessageId(scaleDecoder: scaleDecoder)
        timestamp = try Chat.Timestamp(scaleDecoder: scaleDecoder)
        versioned = try Chat.VersionedNotificationContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try versioned.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.VersionedNotificationContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .v1(Chat.NotificationContentV1(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .v1(content):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.NotificationContentV1: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .stripped(Chat.StrippedContentV1(scaleDecoder: scaleDecoder))
        case 1:
            self = try .full(Chat.RemoteMessageContentV1(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .stripped(content):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .full(content):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.StrippedContentV1: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .text: 0
        case .contactAdded: 3
        case .reacted: 4
        case .reply: 7
        case .dataChannelOffer: 8
        case .chatAccepted: 14
        case .richText: 15
        case .coinageSend: 16
        case .multiChatAccepted: 20
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .text(String(scaleDecoder: scaleDecoder))
        case 3:
            self = .contactAdded
        case 4:
            self = try .reacted(Content.ReactionContent(scaleDecoder: scaleDecoder))
        case 7:
            self = try .reply(Content.ReplyContent(scaleDecoder: scaleDecoder))
        case 8:
            self = try .dataChannelOffer(Content.DataChannelOfferContent(scaleDecoder: scaleDecoder))
        case 14:
            self = try .chatAccepted(Content.ChatAccepted(scaleDecoder: scaleDecoder))
        case 15:
            self = try .richText(Content.RichText(scaleDecoder: scaleDecoder))
        case 16:
            self = try .coinageSend(Coinage(scaleDecoder: scaleDecoder))
        case 20:
            self = try .multiChatAccepted(Content.DeviceChatAccepted(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .text(text):
            try text.encode(scaleEncoder: scaleEncoder)
        case .contactAdded:
            break
        case let .reacted(reaction):
            try reaction.encode(scaleEncoder: scaleEncoder)
        case let .reply(reply):
            try reply.encode(scaleEncoder: scaleEncoder)
        case let .dataChannelOffer(offer):
            try offer.encode(scaleEncoder: scaleEncoder)
        case let .chatAccepted(accepted):
            try accepted.encode(scaleEncoder: scaleEncoder)
        case let .richText(richText):
            try richText.encode(scaleEncoder: scaleEncoder)
        case let .coinageSend(coinage):
            try coinage.encode(scaleEncoder: scaleEncoder)
        case let .multiChatAccepted(accepted):
            try accepted.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.StrippedContentV1.Coinage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        totalValue = try Balance(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try totalValue.encode(scaleEncoder: scaleEncoder)
    }
}
