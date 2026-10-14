import Foundation

/// The paging, jump, refusal and readiness parts of the room screen (P5.3 to P5.7).
extension ChatPresenter {
    func loadOlderMessages() {
        interactor.loadOlderMessages()
    }

    func loadNewerMessages() {
        interactor.loadNewerMessages()
    }

    func jumpToMessage(_ messageId: String) {
        interactor.jump(to: messageId)
    }

    func retryRoom() {
        interactor.retryRoom()
    }

    func didRefuseUserAction(reason: String?) {
        view?.showActionFailure(reason ?? String(localized: .ChatExtension.actionRefused))
    }

    func didJump(to messageId: Chat.MessageId, found: Bool) {
        guard found else {
            return
        }

        view?.scrollToMessage(messageId)
    }
}
