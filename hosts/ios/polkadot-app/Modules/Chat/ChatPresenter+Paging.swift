import Foundation

/// The paging and jump parts of the room screen (P5.3 to P5.7).
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

    func didJump(to messageId: Chat.MessageId, found: Bool) {
        guard found else {
            return
        }

        view?.scrollToMessage(messageId)
    }
}
