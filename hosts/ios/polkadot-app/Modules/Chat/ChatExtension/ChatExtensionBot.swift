import Foundation

protocol ChatExtensionDelegate: AnyObject {
    func didEnableExtensions(_ extensionIds: Set<ChatExtension.Id>)
    func didDisableExtensions(_ extensionIds: Set<ChatExtension.Id>)
}

@MainActor
protocol ChatExtensionDelegateProvidable: AnyObject {
    var delegate: ChatExtensionDelegate? { get set }
}

protocol ChatExtensionBotProtocol: AnyObject, ChatExtending {
    var peerMetadata: Chat.PeerMetadata { get }

    func deliverAutomaticMessages(_ context: ChatExtensionDiscoverContextProtocol)
}

extension ChatExtensionBotProtocol {
    func activeIn(chat: Chat.Id) -> Bool {
        switch chat {
        case let .chatExtension(extId, _):
            extId == identifier
        case .person:
            false
        }
    }
}

class ChatExtensionBot {
    func onTextMessage(
        _: Chat.LocalMessage,
        text _: String,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        fatalError("Method must be overriden")
    }

    func process(
        message: Chat.LocalMessage,
        lastProcessingOutcome: ChatExtension.ProcessingHistoryOutcome,
        context: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        // bots only handle message once
        if lastProcessingOutcome == .previouslyProcessed {
            return .skipped
        }

        // don't process messages from extensions
        if case .chatExtension = message.origin {
            return .skipped
        }

        switch message.content {
        case .contactAdded,
             .leftChat,
             .send,
             .coinageSend,
             .token,
             .reacted,
             .reactionRemoved,
             .edited,
             .chatAccepted,
             .multiChatAccepted,
             .staticTextImageContent,
             .chatRequest,
             .versionedChatRequest,
             .richText,
             .compactedMessages,
             .unsupported:
            return .skipped
        case let .extensionActionResponse(content, _):
            return await onTextMessage(
                message,
                text: content,
                context: context
            )
        case let .reply(content):
            guard let text = content.ownContent.text else {
                return .skipped
            }

            return await onTextMessage(
                message,
                text: text,
                context: context
            )
        case let .text(text):
            return await onTextMessage(
                message,
                text: text,
                context: context
            )
        case .customRendered,
             .file,
             .deviceAdded,
             .deviceRemoved,
             .call:
            return .skipped
        }
    }
}
