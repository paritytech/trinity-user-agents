import Foundation
import PolkadotUI

struct MessageLayoutContext {
    typealias LayoutType = ChatMessageContainerConfiguration.LayoutType

    let layoutType: LayoutType
    let showTimestamp: Bool

    static let standard = MessageLayoutContext(layoutType: .plain, showTimestamp: true)
}

final class MessageLayoutContextCalculator {
    private static let messageGroupingThresholdSeconds: TimeInterval = 60 // 1 minute

    func calculateLayoutContexts(for messages: [Chat.LocalMessage]) -> [String: MessageLayoutContext] {
        var contexts: [String: MessageLayoutContext] = [:]

        for (index, message) in messages.enumerated() {
            guard message.isGroupable else {
                contexts[message.messageId] = .standard
                continue
            }

            let previousGroupable = messages[..<index].last { $0.isIndependentMessageInChat && $0.isGroupable }
            let nextGroupable = messages[(index + 1)...].first { $0.isIndependentMessageInChat && $0.isGroupable }

            let isGroupedWithNext: Bool
            if let next = nextGroupable {
                let sameSender = next.status.isIncoming == message.status.isIncoming
                let timeDiff = abs(
                    Date.fromChatTimestamp(next.timestamp)
                        .timeIntervalSince(Date.fromChatTimestamp(message.timestamp))
                )
                isGroupedWithNext = sameSender && timeDiff < Self.messageGroupingThresholdSeconds
            } else {
                isGroupedWithNext = false
            }

            let isGroupedWithPrev: Bool
            if let prev = previousGroupable {
                let sameSender = prev.status.isIncoming == message.status.isIncoming
                let timeDiff = abs(
                    Date.fromChatTimestamp(message.timestamp)
                        .timeIntervalSince(Date.fromChatTimestamp(prev.timestamp))
                )
                isGroupedWithPrev = sameSender && timeDiff < Self.messageGroupingThresholdSeconds
            } else {
                isGroupedWithPrev = false
            }

            let layoutType: MessageLayoutContext.LayoutType =
                switch (isGroupedWithPrev, isGroupedWithNext) {
                case (false, false): .plain
                case (false, true): .groupedTop
                case (true, true): .groupedMiddle
                case (true, false): .groupedBottom
                }

            let showTimestamp = !isGroupedWithNext

            contexts[message.messageId] = MessageLayoutContext(
                layoutType: layoutType,
                showTimestamp: showTimestamp
            )
        }

        return contexts
    }
}
