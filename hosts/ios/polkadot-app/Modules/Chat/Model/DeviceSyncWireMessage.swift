import Foundation
import SubstrateSdk

// MARK: - Model

extension Chat {
    struct DeviceSyncWireMessage: Equatable {
        static let peerIdLength = 32

        let remote: RemoteMessage
        let peerId: Data
        let status: DeviceSyncLocalStatus
        let order: UInt64
    }
}

// MARK: - Local Mapping

extension Chat.DeviceSyncWireMessage {
    init?(from local: Chat.LocalMessage) {
        // Only contact chats are supported for sync
        guard case let .person(accountId) = local.chatId else {
            return nil
        }

        guard let remoteContent = local.content.toRemoteMessageContent() else {
            return nil
        }

        let remoteMessage = Chat.RemoteMessage(
            messageId: local.messageId,
            timestamp: local.timestamp,
            versioned: .v1(Chat.RemoteMessageContentV1(content: remoteContent))
        )

        remote = remoteMessage
        peerId = accountId
        status = Chat.DeviceSyncLocalStatus(from: local.status)
        order = local.timestamp
    }

    func toLocal() -> Chat.LocalMessage? {
        guard let content = Chat.LocalMessage.Content(remote: remote.versioned) else {
            return nil
        }

        let origin: Chat.LocalMessage.Origin =
            switch status {
            case .outgoing:
                .user
            case .incoming:
                .contact(peerId)
            }

        return Chat.LocalMessage(
            messageId: remote.messageId,
            chatId: .person(peerId),
            origin: origin,
            creationSource: .deviceSync,
            status: status.toLocal(),
            timestamp: remote.timestamp,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}

// MARK: - RemoteMessageContentV1.MessageContent from LocalMessage.Content

extension Chat.LocalMessage.Content {
    // swiftlint:disable:next cyclomatic_complexity
    func toRemoteMessageContent() -> Chat.RemoteMessageContentV1.MessageContent? {
        switch self {
        case let .text(text): .text(text)
        case let .token(content): .token(content)
        case let .send(content): .send(content)
        case .contactAdded: .contactAdded
        case .leftChat: .leftChat
        case let .reacted(content): .reacted(content)
        case let .reactionRemoved(content): .reactionRemoved(content)
        case let .edited(content): .edited(content)
        case let .reply(content): .reply(content)
        case let .chatAccepted(content): .chatAccepted(content)
        case let .deviceAdded(content): .deviceAdded(content)
        case let .deviceRemoved(content): .deviceRemoved(content)
        case let .multiChatAccepted(content): .multiChatAccepted(content)
        case let .compactedMessages(content): .compactedMessages(content)
        case let .richText(richText): richText.toRemote().map { .richText($0) }
        case let .coinageSend(transfer):
            .coinageSend(.init(totalValue: transfer.totalValue, coinKeys: transfer.coinKeys))
        case let .call(payload): payload.toRemoteMessageContent()
        case .unsupported,
             .staticTextImageContent,
             .extensionActionResponse,
             .customRendered,
             .chatRequest,
             .versionedChatRequest,
             .file:
            nil
        }
    }
}

private extension Chat.LocalMessage.Content.CallSignalingPayload {
    func toRemoteMessageContent() -> Chat.RemoteMessageContentV1.MessageContent {
        switch self {
        case let .offer(content): .dataChannelOffer(content)
        case let .answer(content): .dataChannelAnswer(content)
        case let .candidates(content): .dataChannelCandidates(content)
        case let .closed(content): .dataChannelClosed(content)
        }
    }
}

// MARK: - ScaleCodable

extension Chat.DeviceSyncWireMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        remote = try Chat.RemoteMessage(scaleDecoder: scaleDecoder)
        peerId = try scaleDecoder.readAndConfirm(count: Self.peerIdLength)
        status = try Chat.DeviceSyncLocalStatus(scaleDecoder: scaleDecoder)
        order = try UInt64(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try remote.encode(scaleEncoder: scaleEncoder)
        scaleEncoder.appendRaw(data: peerId)
        try status.encode(scaleEncoder: scaleEncoder)
        try order.encode(scaleEncoder: scaleEncoder)
    }
}
