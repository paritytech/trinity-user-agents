import Foundation

extension ChatPresenter: ChatMoreActionsDelegate {
    func didConfirmLeaveChat() {
        interactor.leaveChat()
    }

    func didConfirmBlockUser() {
        interactor.blockUser()
    }
}

extension ChatPresenter: ChatAttachmentPreviewDelegate {
    func attachmentPreview(didComplete text: String, attachment: ProcessedAttachment) {
        interactor.send(text: text, attachments: [attachment], replyToMessageId: nil)
    }
}
