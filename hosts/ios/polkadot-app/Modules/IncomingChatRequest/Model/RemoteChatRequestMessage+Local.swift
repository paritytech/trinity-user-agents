import Foundation

extension Chat.RequestMessage {
    init?(localMessage: Chat.LocalMessage) {
        let requestContent: Chat.VersionedRequestContent

        switch localMessage.content {
        case let .chatRequest(contentModel):
            requestContent = .v1(contentModel)
        case let .versionedChatRequest(versionedContent):
            requestContent = versionedContent
        default:
            return nil
        }

        self.init(
            messageId: localMessage.messageId,
            timestamp: localMessage.timestamp,
            content: requestContent
        )
    }
}
