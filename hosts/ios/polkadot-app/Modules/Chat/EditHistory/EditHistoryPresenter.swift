import Foundation
import PolkadotUI

final class EditHistoryPresenter {
    weak var view: EditHistoryViewProtocol?
    let interactor: EditHistoryInteractorInputProtocol

    private let messageId: String
    private let timestampFormatter: TimestampFormatting

    init(
        messageId: String,
        timestampFormatter: TimestampFormatting,
        interactor: EditHistoryInteractorInputProtocol
    ) {
        self.messageId = messageId
        self.timestampFormatter = timestampFormatter
        self.interactor = interactor
    }
}

extension EditHistoryPresenter: EditHistoryPresenterProtocol {
    func setup() {
        interactor.fetchEditHistory()
    }
}

extension EditHistoryPresenter: EditHistoryInteractorOutputProtocol {
    func didReceive(result: EditHistoryResult) {
        let currentMessage = EditHistoryViewModel.CurrentMessage(
            text: result.currentMessage.text,
            timestamp: result.currentMessage.timestamp,
            outboxStatus: result.currentMessage.outboxStatus
        )

        let viewModelItems = result.historyItems.enumerated().map { index, item in
            EditHistoryViewModel.HistoryItem(
                id: "\(messageId)_\(index)",
                diffParts: item.diffParts,
                timestamp: item.timestamp
            )
        }

        let viewModel = EditHistoryViewModel(
            currentMessage: currentMessage,
            historyItems: viewModelItems,
            originalTimestamp: result.originalTimestamp,
            isOutgoing: result.isOutgoing
        )
        view?.didReceive(viewModel: viewModel)
    }
}
