import Foundation
import PolkadotUI

protocol EditHistoryViewProtocol: AnyObject {
    func didReceive(viewModel: EditHistoryViewModel)
}

@MainActor
protocol EditHistoryPresenterProtocol: AnyObject {
    func setup()
}

protocol EditHistoryInteractorInputProtocol: AnyObject {
    func fetchEditHistory()
}

@MainActor
protocol EditHistoryInteractorOutputProtocol: AnyObject {
    func didReceive(result: EditHistoryResult)
}

struct EditHistoryResult {
    let currentMessage: EditHistoryCurrentMessage
    let historyItems: [EditHistoryHistoryItem]
    let originalTimestamp: Date
    let isOutgoing: Bool
}

struct EditHistoryCurrentMessage {
    let text: String
    let timestamp: Date
    let outboxStatus: ChatMessageStatusViewConfiguration.OutboxStatus?
}

struct EditHistoryHistoryItem {
    let diffParts: [TextDiffCalculator.DiffPart]
    let timestamp: Date
}
