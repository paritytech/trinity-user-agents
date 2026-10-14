import Foundation
import PolkadotUI
import SubstrateSdk
import PhotosUI
import UIKit
import UIKitExt

protocol ChatViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: ChatViewLayout.ViewModel)
    func didReceive(footer: (any HashableContentConfiguration)?)
    func didReceive(callActions: [ChatCallType])
    func didReceive(contactMenu: UIMenu?)
    func showReply(messageId: String, username: String, text: String)
    func showEdit(messageId: String, currentText: String)
    func showReactionDetails(viewModel: ReactionDetailsViewModel)
}

@MainActor
protocol ChatPresenterProtocol: AnyObject {
    var chatId: Chat.Id? { get }

    func setup()
    func send(text: String, replyToMessageId: String?)
    func sendEdit(messageId: String, newText: String)
    func viewWillAppear()
    func viewWillDisappear()
    func readTillMessage(identifier: String)

    func makeTransfer()
    func showAttachmentSelection()
    func startCall(_ callType: ChatCallType)
    func onReply(for messageId: String)
    func onScrollToBottom()
    func onScrollToReaction()
}

protocol ChatInteractorInputProtocol: AnyObject {
    func setup()

    func send(
        text: String?,
        attachments: [ProcessedAttachment]?,
        replyToMessageId: String?
    )

    func sendEdit(messageId: String, newText: String)
    func notifyViewAppeared()
    func notifyViewDisappeared()
    func readAllBefore(identifier: Chat.MessageId)
    func readMessages(_ identifiers: [Chat.MessageId])
    func toggleReaction(messageId: String, emoji: String)
    func leaveChat()
    func blockUser()
    func unblockUser()
    func acceptChatRequest()
    func declineChatRequest()
    func processAction(_ action: Chat.Action)
    func isMicrophoneDenied() -> Bool
}

@MainActor
protocol ChatInteractorOutputProtocol: AnyObject {
    func didReceive(metadata: MessageListMetadata)

    func didReceive(listModel: MessageListModel)

    func didReceiveFooter(_ footer: (any HashableContentConfiguration)?)

    func didLeftChat()

    func didBlockUser()

    func didDeclineChatRequest()
}

@MainActor
protocol ChatWireframeProtocol: AnyObject, AlertPresentable {
    func showSendAsset(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata
    )

    func makeContactActionsMenu(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        actions: [Chat.PeerAction],
        delegate: ChatMoreActionsDelegate
    ) -> UIMenu

    func showCall(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        callType: ChatCallType
    )

    func presentMicrophoneAccessDenied(from view: ControllerBackedProtocol?)

    func showEditHistory(
        from view: ControllerBackedProtocol?,
        messageId: String
    )

    func dismissChat(from view: ControllerBackedProtocol?)

    func showDocument(at url: URL)

    func showImagePicker(
        from view: ControllerBackedProtocol?,
        onAttachmentSelected: @escaping (([ChatAttachmentProviding]) -> Void)
    )

    func showAttachmentSelection(
        from view: ControllerBackedProtocol?,
        providers: [ChatAttachmentProviding],
        onComplete: @escaping (ProcessedAttachmentResult) -> Void
    )

    func showAttachmentPreview(
        from view: ControllerBackedProtocol?,
        attachment: Chat.LocalMessage.Content.Attachment
    )
}

protocol ImagePickerDelegate: AnyObject {
    func imagePicker(didSelectImageAt url: URL)
    func imagePickerDidCancel()
}

protocol ChatAttachmentPreviewDelegate: AnyObject {
    func attachmentPreview(didComplete text: String, attachment: ProcessedAttachment)
}

protocol ChatMoreActionsDelegate: AnyObject {
    func didConfirmLeaveChat()
    func didConfirmBlockUser()
}
