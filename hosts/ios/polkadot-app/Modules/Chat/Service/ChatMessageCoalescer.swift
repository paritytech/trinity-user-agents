import MessageExchangeKit

struct ChatMessageCoalescer: MessageCoalescing {
    typealias Message = Chat.OpaqueMessage

    func canCoalesce(_ message: Chat.OpaqueMessage) -> Bool {
        seenContent(of: message) != nil
    }

    func decision(
        for newMessage: Chat.OpaqueMessage,
        against pendingMessage: Chat.OpaqueMessage
    ) -> MessageCoalescingDecision {
        guard
            let newSeen = seenContent(of: newMessage),
            let pendingSeen = seenContent(of: pendingMessage)
        else {
            return .append
        }

        return newSeen == pendingSeen ? .ignore : .replace
    }
}

private extension ChatMessageCoalescer {
    func seenContent(of message: Chat.OpaqueMessage) -> ChatRemoteMessageContent.SeenContent? {
        guard case let .seen(content) = message.remoteMessage.versioned.ensureV1()?.content else {
            return nil
        }

        return content
    }
}
