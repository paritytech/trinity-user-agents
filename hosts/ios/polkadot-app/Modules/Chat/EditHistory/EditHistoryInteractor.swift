import Foundation
import Operation_iOS
import PolkadotUI

final class EditHistoryInteractor {
    weak var presenter: EditHistoryInteractorOutputProtocol?

    private let messageId: String
    private let repository: AnyDataProviderRepository<Chat.LocalMessage>
    private let diffCalculator: TextDiffCalculator

    init(
        messageId: String,
        repository: AnyDataProviderRepository<Chat.LocalMessage>,
        diffCalculator: TextDiffCalculator = TextDiffCalculator()
    ) {
        self.messageId = messageId
        self.repository = repository
        self.diffCalculator = diffCalculator
    }
}

extension EditHistoryInteractor: EditHistoryInteractorInputProtocol {
    func fetchEditHistory() {
        Task {
            guard let messages = try? await repository.fetchAllOperation(with: RepositoryFetchOptions())
                .asyncExecute() else {
                return
            }

            guard let originalMessage = messages.first(where: { $0.identifier == messageId }),
                  let originalText = originalMessage.content.originalMessageText else {
                return
            }

            let outboxStatus = outboxStatus(for: originalMessage.status)

            var textItems: [(text: String, timestamp: Date)] = messages.compactMap { message -> (String, Date)? in
                switch message.content {
                case let .edited(editedContent):
                    guard let text = editedContent.newContent.text else { return nil }
                    return (text, Date.fromChatTimestamp(message.timestamp))
                default:
                    return nil
                }
            }

            textItems.append((originalText, Date.fromChatTimestamp(originalMessage.timestamp)))

            guard let currentItem = textItems.first else { return }

            let currentMessage = EditHistoryCurrentMessage(
                text: currentItem.text,
                timestamp: currentItem.timestamp,
                outboxStatus: outboxStatus
            )

            var historyItems: [EditHistoryHistoryItem] = []
            for index in 0 ..< textItems.count - 1 {
                let currentText = textItems[index].text
                let previousText = textItems[index + 1].text
                let diffParts = diffCalculator.computeDiff(from: previousText, to: currentText)

                historyItems.append(EditHistoryHistoryItem(
                    diffParts: diffParts,
                    timestamp: textItems[index].timestamp
                ))
            }

            let originalTimestamp = textItems.last?.timestamp ?? currentItem.timestamp

            let result = EditHistoryResult(
                currentMessage: currentMessage,
                historyItems: historyItems,
                originalTimestamp: originalTimestamp,
                isOutgoing: originalMessage.status.isOutgoing
            )

            await presenter?.didReceive(result: result)
        }
    }
}

private extension EditHistoryInteractor {
    func outboxStatus(for status: Chat.LocalMessage.Status) -> ChatMessageStatusViewConfiguration.OutboxStatus? {
        switch status {
        case let .outgoing(outgoingStatus):
            outgoingStatus.outboxStatus
        case .incoming:
            nil
        }
    }
}

private extension Chat.LocalMessage.Status.OutgoingStatus {
    var outboxStatus: ChatMessageStatusViewConfiguration.OutboxStatus {
        switch self {
        case .new: .pending
        case .sent: .sent
        case .delivered: .delivered
        case .failed: .failed
        }
    }
}
