import Foundation

extension Chat.RemoteMessage {
    var isForCallProtocol: Bool {
        switch versioned.ensureV1()?.content {
        case .dataChannelOffer,
             .dataChannelAnswer,
             .dataChannelCandidates,
             .dataChannelClosed:
            true
        default:
            false
        }
    }

    static func newMessage(with content: Chat.RemoteMessageContentV1.MessageContent) -> Chat.RemoteMessage {
        Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: Date().toChatTimestamp(),
            versioned: .v1(.init(content: content))
        )
    }
}
