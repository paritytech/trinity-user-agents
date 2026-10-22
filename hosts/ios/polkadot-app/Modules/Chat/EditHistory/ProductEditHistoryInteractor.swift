import Foundation
import PolkadotUI
import TrUAPIHost

/// The edit history of a message of a room that the product keeps (`edit_history`). The product gives every text of
/// the message, oldest first, and the screen draws it as the edit history of the native chat.
final class ProductEditHistoryInteractor {
    weak var presenter: EditHistoryInteractorOutputProtocol?

    private let request: ProductChatEditHistoryQueryRequest
    private let status: Chat.LocalMessage.Status
    private let room: ProductRoomInteracting
    private let diffCalculator: TextDiffCalculator
    private let logger: LoggerProtocol

    init(
        request: ProductChatEditHistoryQueryRequest,
        status: Chat.LocalMessage.Status,
        room: ProductRoomInteracting,
        diffCalculator: TextDiffCalculator = TextDiffCalculator(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.request = request
        self.status = status
        self.room = room
        self.diffCalculator = diffCalculator
        self.logger = logger
    }
}

extension ProductEditHistoryInteractor: EditHistoryInteractorInputProtocol {
    func fetchEditHistory() {
        Task {
            do {
                let item = try await room.editHistory(request)
                guard let result = result(of: item.revisions) else { return }
                await presenter?.didReceive(result: result)
            } catch {
                logger.warning("Room \(request.roomId) gave no edit history: \(error)")
            }
        }
    }
}

extension ProductEditHistoryInteractor {
    /// The history as the native edit history draws it: the newest text first, and the original text last.
    func result(of revisions: [ChatMessageRevision]) -> EditHistoryResult? {
        let textItems: [(text: String, timestamp: Date)] = revisions
            .reversed()
            .map { ($0.text ?? "", Date.fromChatTimestamp($0.editedAt)) }

        guard let currentItem = textItems.first else { return nil }

        let historyItems = (0 ..< max(textItems.count - 1, 0)).map { index in
            EditHistoryHistoryItem(
                diffParts: diffCalculator.computeDiff(from: textItems[index + 1].text, to: textItems[index].text),
                timestamp: textItems[index].timestamp
            )
        }

        let outboxStatus: ChatMessageStatusViewConfiguration.OutboxStatus? =
            switch status {
            case let .outgoing(outgoingStatus): outgoingStatus.configurationStatus()
            case .incoming: nil
            }

        return EditHistoryResult(
            currentMessage: EditHistoryCurrentMessage(
                text: currentItem.text,
                timestamp: currentItem.timestamp,
                outboxStatus: outboxStatus
            ),
            historyItems: historyItems,
            originalTimestamp: textItems.last?.timestamp ?? currentItem.timestamp,
            isOutgoing: status.isOutgoing
        )
    }
}
