import Foundation
import PolkadotUI

struct EditHistoryViewModel {
    typealias DiffPart = TextDiffCalculator.DiffPart
    typealias OutboxStatus = ChatMessageStatusViewConfiguration.OutboxStatus

    struct HistoryItem: Identifiable {
        let id: String
        let diffParts: [DiffPart]
        let timestamp: Date
    }

    struct CurrentMessage {
        let text: String
        let timestamp: Date
        let outboxStatus: OutboxStatus?
    }

    let currentMessage: CurrentMessage
    let historyItems: [HistoryItem]
    let originalTimestamp: Date
    let isOutgoing: Bool
}
