import Foundation
import PolkadotUI
import Products

extension ChatPresenter {
    @MainActor
    func didFailReclaim(messageId _: Chat.MessageId, error: Error) {
        if !wireframe.present(error: error, from: view) {
            wireframe.present(
                message: String(localized: .Common.errorMessage),
                title: nil,
                closeAction: String(localized: .Common.close),
                from: view
            )
        }
    }

    @MainActor
    func handleReclaimTransfer(for messageId: Chat.MessageId) {
        guard
            let message = listModel?.getMessage(by: messageId),
            case let .coinageSend(content) = message.content,
            content.isReclaimable
        else {
            return
        }

        interactor.reclaimTransfer(messageId: messageId, coinKeys: content.coinKeys)
    }
}
