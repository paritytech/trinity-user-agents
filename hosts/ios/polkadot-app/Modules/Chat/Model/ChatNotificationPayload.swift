import Foundation
import SubstrateSdk

extension Chat {
    /// Push-only wire type carried in the relay's `message` field: `messageId ‖ timestamp ‖ version ‖ kind ‖ content`.
    /// Full payloads are persisted by the notification extension; stripped payloads exist
    /// only to render the notification and must never be saved.
    struct NotificationPayload: Equatable {
        let messageId: MessageId
        let timestamp: Timestamp
        let versioned: VersionedNotificationContent

        init(messageId: MessageId, timestamp: Timestamp, versioned: VersionedNotificationContent) {
            self.messageId = messageId
            self.timestamp = timestamp
            self.versioned = versioned
        }

        init(full message: RemoteMessage) throws {
            guard let content = message.versioned.ensureV1() else {
                throw RemoteCodingError.unsupportedEncoding
            }

            self.init(messageId: message.messageId, timestamp: message.timestamp, versioned: .v1(.full(content)))
        }

        /// Nil when the message has no stripped form.
        init?(stripped message: RemoteMessage) {
            guard
                let content = message.versioned.ensureV1(),
                let stripped = StrippedContentV1(content.content)
            else {
                return nil
            }

            self.init(messageId: message.messageId, timestamp: message.timestamp, versioned: .v1(.stripped(stripped)))
        }

        var fullMessage: RemoteMessage? {
            guard case let .v1(.full(content)) = versioned else {
                return nil
            }

            return RemoteMessage(messageId: messageId, timestamp: timestamp, versioned: .v1(content))
        }

        /// A call offer rings whether it arrived full or stripped.
        var dataChannelOffer: RemoteMessageContentV1.MessageContent.DataChannelOfferContent? {
            switch versioned {
            case let .v1(.full(content)):
                if case let .dataChannelOffer(offer) = content.content {
                    return offer
                }
                return nil
            case let .v1(.stripped(.dataChannelOffer(offer))):
                return offer
            case .v1(.stripped):
                return nil
            }
        }
    }

    enum VersionedNotificationContent: Equatable {
        // swiftlint:disable:next identifier_name
        case v1(NotificationContentV1)
    }

    enum NotificationContentV1: Equatable {
        case stripped(StrippedContentV1)
        case full(RemoteMessageContentV1)
    }

    /// Only coinage loses data (`coinKeys`); the others carry the same bytes as their full form.
    enum StrippedContentV1: Equatable {
        typealias Content = RemoteMessageContentV1.MessageContent

        struct Coinage: Equatable {
            let totalValue: Balance
        }

        case text(String)
        case contactAdded
        case reacted(Content.ReactionContent)
        case reply(Content.ReplyContent)
        case dataChannelOffer(Content.DataChannelOfferContent)
        case chatAccepted(Content.ChatAccepted)
        case richText(Content.RichText)
        case coinageSend(Coinage)
        case multiChatAccepted(Content.DeviceChatAccepted)
    }
}

extension Chat.StrippedContentV1 {
    init?(_ content: Content) {
        switch content {
        case let .text(text):
            self = .text(text)
        case .contactAdded:
            self = .contactAdded
        case let .reacted(reaction):
            self = .reacted(reaction)
        case let .reply(reply):
            self = .reply(reply)
        case let .dataChannelOffer(offer):
            self = .dataChannelOffer(offer)
        case let .chatAccepted(accepted):
            self = .chatAccepted(accepted)
        case let .richText(richText):
            self = .richText(richText)
        case let .coinageSend(coinage):
            self = .coinageSend(Coinage(totalValue: coinage.totalValue))
        case let .multiChatAccepted(accepted):
            self = .multiChatAccepted(accepted)
        case .token,
             .send,
             .reactionRemoved,
             .dataChannelAnswer,
             .dataChannelCandidates,
             .dataChannelClosed,
             .edited,
             .leftChat,
             .deviceAdded,
             .deviceRemoved,
             .compactedMessages:
            return nil
        }
    }

    /// The full-form content for everything except coinage, which has no full form without its keys.
    var regularContent: Content? {
        switch self {
        case let .text(text):
            .text(text)
        case .contactAdded:
            .contactAdded
        case let .reacted(reaction):
            .reacted(reaction)
        case let .reply(reply):
            .reply(reply)
        case let .dataChannelOffer(offer):
            .dataChannelOffer(offer)
        case let .chatAccepted(accepted):
            .chatAccepted(accepted)
        case let .richText(richText):
            .richText(richText)
        case .coinageSend:
            nil
        case let .multiChatAccepted(accepted):
            .multiChatAccepted(accepted)
        }
    }
}
