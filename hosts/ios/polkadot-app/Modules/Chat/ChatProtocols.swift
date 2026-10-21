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
    /// Scrolls to the message now, or when it arrives in the window.
    func scrollToMessage(_ messageId: String)
    /// Shows why an action of the user did not happen.
    func showActionFailure(_ message: String)
    /// Puts a refused text in the input again.
    func restoreInput(text: String)
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
    /// The user scrolled near the oldest message of the window.
    func loadOlderMessages()
    /// The user scrolled near the newest message of the window.
    func loadNewerMessages()
    /// The user tapped a reply quote whose message is not in the window.
    func jumpToMessage(_ messageId: String)
    /// The user asked the product of the room again after it did not answer.
    func retryRoom()
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
    /// Sends a failed message of a product-source room again through `user_action` (P5.5).
    func retrySend(messageId: Chat.MessageId)
    func deleteMessage(messageId: Chat.MessageId)
    /// Asks the message source for the page before the window.
    func loadOlderMessages()
    /// Asks the message source for the page after the window.
    func loadNewerMessages()
    /// Asks the message source for pages until the message is in the window.
    func jump(to messageId: Chat.MessageId)
    /// The room screen shows the room: a product room holds its product (02-DESIGN §5.2).
    func openRoom()
    /// The room screen does not show the room anymore: a product room releases its product.
    func closeRoom()
    /// Opens the room again and asks for the newest page, after the product did not answer (P5.7).
    func retryRoom()
}

@MainActor
protocol ChatInteractorOutputProtocol: AnyObject {
    func didReceive(metadata: MessageListMetadata)

    func didReceive(listModel: MessageListModel)

    func didReceiveFooter(_ footer: (any HashableContentConfiguration)?)

    func didLeftChat()

    func didBlockUser()

    func didDeclineChatRequest()

    /// The chat of the screen went away, for example because its product deleted the room.
    func didRemoveChat()

    /// A jump ended. `found` is false when the room does not hold the message.
    func didJump(to messageId: Chat.MessageId, found: Bool)

    /// The product refused an action of the user. The reason is for the user, when the product gave one.
    func didRefuseUserAction(reason: String?)

    /// The product refused a new message, a reply or an edit with this text. The text goes back into the input.
    func didRefuseSend(_ refused: ChatRefusedSend)
}

extension ChatInteractorOutputProtocol {
    func didJump(to _: Chat.MessageId, found _: Bool) {}

    func didRefuseUserAction(reason _: String?) {}

    func didRefuseSend(_ refused: ChatRefusedSend) {
        didRefuseUserAction(reason: refused.reason)
    }
}

/// A text that the product refused, with what the user did with it.
struct ChatRefusedSend: Equatable {
    let text: String
    /// The message that the text replied to.
    var replyToMessageId: String?
    /// The message that the text edited.
    var editedMessageId: String?
    let reason: String?
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

    /// Asks the user to confirm the delete of a message.
    func confirmDeleteMessage(from view: ControllerBackedProtocol?, onConfirm: @escaping () -> Void)

    func showDocument(at url: URL)

    /// Opens an `http` or `https` address in the in-app browser after the user confirms.
    func confirmOpenLink(_ url: URL, from view: ControllerBackedProtocol?)

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
