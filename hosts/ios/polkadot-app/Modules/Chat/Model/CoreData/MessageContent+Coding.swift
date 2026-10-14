import Foundation
import SubstrateSdk
import UIKit.UIImage

extension Chat.LocalMessage.Content: ScaleCodable {
    // swiftlint:disable:next cyclomatic_complexity
    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch ContentType(rawValue: idx) {
        case .text:
            self = try .text(String(scaleDecoder: scaleDecoder))
        case .token:
            let content = try Chat.RemoteTokenContent(scaleDecoder: scaleDecoder)
            self = .token(content)
        case .send:
            let content = try Chat.RemoteMessageContentV1.MessageContent.SendContent.Legacy(scaleDecoder: scaleDecoder)
            self = .send(content)
        case .contactAdded:
            self = .contactAdded
        case .leftChat:
            self = .leftChat
        case .reacted:
            let content = try Chat.RemoteMessageContentV1.MessageContent.ReactionContent(scaleDecoder: scaleDecoder)
            self = .reacted(content)
        case .reactionRemoved:
            let content = try Chat.RemoteMessageContentV1.MessageContent.ReactionContent(scaleDecoder: scaleDecoder)
            self = .reactionRemoved(content)
        case .edited:
            let content = try Chat.RemoteMessageContentV1.MessageContent.EditedContent(scaleDecoder: scaleDecoder)
            self = .edited(content)
        case .reply:
            let content = try Chat.RemoteMessageContentV1.MessageContent.ReplyContent(scaleDecoder: scaleDecoder)
            self = .reply(content)
        case .chatAccepted:
            let content = try Chat.RemoteMessageContentV1.MessageContent.ChatAccepted(scaleDecoder: scaleDecoder)
            self = .chatAccepted(content)
        case .unsupported:
            if scaleDecoder.remained > 0 {
                let optData = try ScaleOption<Data>(scaleDecoder: scaleDecoder).value
                self = .unsupported(optData)
            } else {
                self = .unsupported(nil)
            }
        case .staticImageText:
            let text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
            let imageData = try Data(scaleDecoder: scaleDecoder)
            self = .staticTextImageContent(StaticTextImageContent(text: text, media: UIImage(data: imageData)!))
        case .extensionActionResponse:
            let response = try String(scaleDecoder: scaleDecoder)
            let actionId = try String(scaleDecoder: scaleDecoder)
            self = .extensionActionResponse(response, actionId)
        case .chatRequest:
            let content = try Chat.RequestContentV1(scaleDecoder: scaleDecoder)
            self = .chatRequest(content)
        case .versionedChatRequest:
            let content = try Chat.VersionedRequestContent(scaleDecoder: scaleDecoder)
            self = .versionedChatRequest(content)
        case .customRendered:
            let content = try CustomRenderedData(scaleDecoder: scaleDecoder)
            self = .customRendered(content)
        case .file:
            self = try .file(.init(scaleDecoder: scaleDecoder))
        case .richText:
            let content = try RichText(scaleDecoder: scaleDecoder)
            self = .richText(content)
        case .coinageSend:
            let remote = try Chat.RemoteMessageContentV1.MessageContent.SendContent.Coinage(scaleDecoder: scaleDecoder)
            self = .coinageSend(Transfer(remote))
        case .deviceAdded:
            let content = try Chat.RemoteMessageContentV1.MessageContent.DeviceAddedContent(scaleDecoder: scaleDecoder)
            self = .deviceAdded(content)
        case .deviceRemoved:
            let content = try Chat.RemoteMessageContentV1.MessageContent
                .DeviceRemovedContent(scaleDecoder: scaleDecoder)
            self = .deviceRemoved(content)
        case .multiChatAccepted:
            let content = try Chat.RemoteMessageContentV1.MessageContent
                .DeviceChatAccepted(scaleDecoder: scaleDecoder)
            self = .multiChatAccepted(content)
        case .call:
            self = try .call(Chat.LocalMessage.Content.CallSignalingPayload(scaleDecoder: scaleDecoder))
        case .compactedMessages:
            self = try .compactedMessages(
                ChatRemoteMessageContent.CompactedMessagesContent(scaleDecoder: scaleDecoder)
            )
        case .none:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func encode(scaleEncoder: any ScaleEncoding) throws {
        try contentType.rawValue.encode(scaleEncoder: scaleEncoder)
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
        case let .edited(editedContent):
            try editedContent.encode(scaleEncoder: scaleEncoder)
        case let .reply(replyContent):
            try replyContent.encode(scaleEncoder: scaleEncoder)
        case let .staticTextImageContent(content):
            try ScaleOption(value: content.text).encode(scaleEncoder: scaleEncoder)
            try content.media.pngData()!.encode(scaleEncoder: scaleEncoder)
        case let .extensionActionResponse(content, action):
            try content.encode(scaleEncoder: scaleEncoder)
            try action.encode(scaleEncoder: scaleEncoder)
        case let .chatAccepted(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .chatRequest(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .versionedChatRequest(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .customRendered(data):
            try data.encode(scaleEncoder: scaleEncoder)
        case let .unsupported(optData):
            try ScaleOption(value: optData).encode(scaleEncoder: scaleEncoder)
        case let .file(file):
            try file.encode(scaleEncoder: scaleEncoder)
        case let .richText(richText):
            try richText.encode(scaleEncoder: scaleEncoder)
        case let .coinageSend(content):
            let remote = Chat.RemoteMessageContentV1.MessageContent.SendContent.Coinage(
                totalValue: content.totalValue,
                coinKeys: content.coinKeys
            )
            try remote.encode(scaleEncoder: scaleEncoder)
        case let .deviceAdded(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .deviceRemoved(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .multiChatAccepted(content):
            try content.encode(scaleEncoder: scaleEncoder)
        case let .call(payload):
            try payload.encode(scaleEncoder: scaleEncoder)
        case let .compactedMessages(content):
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.LocalMessage.Content.CallSignalingPayload: ScaleCodable {
    private enum Tag: UInt8 {
        case offer = 0
        case answer = 1
        case candidates = 2
        case closed = 3
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let tagValue = try UInt8(scaleDecoder: scaleDecoder)
        switch Tag(rawValue: tagValue) {
        case .offer:
            self = try .offer(.init(scaleDecoder: scaleDecoder))
        case .answer:
            self = try .answer(.init(scaleDecoder: scaleDecoder))
        case .candidates:
            self = try .candidates(.init(scaleDecoder: scaleDecoder))
        case .closed:
            self = try .closed(.init(scaleDecoder: scaleDecoder))
        case .none:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .offer(content):
            try Tag.offer.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .answer(content):
            try Tag.answer.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .candidates(content):
            try Tag.candidates.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .closed(content):
            try Tag.closed.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}
