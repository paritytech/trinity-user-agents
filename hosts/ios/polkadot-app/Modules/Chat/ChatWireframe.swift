import Foundation
import UIKit
import SubstrateSdk
import PhotosUI
import UIKitExt
import ChainRegistry

@MainActor
final class ChatWireframe: ChatWireframeProtocol, ApplicationSettingsPresentable {
    let chainAsset: ChainAsset
    let flowState: ChatFlowState
    let documentAdapter: DocumentPreviewPresenting
    let uploadStore: AttachmentStoring
    let downloadStore: AttachmentStoring
    let logger: LoggerProtocol
    let videoPreviewPlayerFactory: VideoPreviewPlayerFactoryProtocol

    private var onAttachmentSelected: (([ChatAttachmentProviding]) -> Void)?

    var mediaPreviewActiveDataSources: [PhotoPreviewDataSource] = []

    init(
        chainAsset: ChainAsset,
        flowState: ChatFlowState,
        documentAdapter: DocumentPreviewPresenting,
        uploadStore: AttachmentStoring,
        downloadStore: AttachmentStoring,
        videoPreviewPlayerFactory: VideoPreviewPlayerFactoryProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chainAsset = chainAsset
        self.flowState = flowState
        self.documentAdapter = documentAdapter
        self.uploadStore = uploadStore
        self.downloadStore = downloadStore
        self.videoPreviewPlayerFactory = videoPreviewPlayerFactory
        self.logger = logger
    }

    func showSendAsset(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata
    ) {
        guard
            case let .person(accountId) = chatMetadata.chatId,
            let destination = TransferAmountViewFactory.createChatTransfer(
                for: chainAsset,
                recipient: .init(accountId: accountId, username: chatMetadata.peerMetadata.name),
                coinageService: flowState.coinageService
            )
        else {
            return
        }

        let navigation = AppNavigationController(rootViewController: destination.controller)
        navigation.modalPresentationStyle = .fullScreen
        view?.controller.present(navigation, animated: true)
    }

    func presentMicrophoneAccessDenied(from view: ControllerBackedProtocol?) {
        let viewModel = AlertPresentableViewModel(
            title: String(localized: .chatCallMicAccessTitle),
            message: String(localized: .chatCallMicAccessMessage),
            actions: [
                AlertPresentableAction(title: String(localized: .Common.openSettings)) { [weak self] in
                    self?.openApplicationSettings()
                }
            ],
            closeActionTitle: String(localized: .Common.notNow)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }

    func showCall(
        from _: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        callType: ChatCallType
    ) {
        guard case let .person(accountId) = chatMetadata.chatId else {
            return
        }

        flowState.callCoordinator.initiateCall(
            with: CallPeer(
                name: chatMetadata.peerMetadata.name,
                accountId: accountId
            ),
            callType: callType
        )
    }

    func makeContactActionsMenu(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        actions: [Chat.PeerAction],
        delegate: ChatMoreActionsDelegate
    ) -> UIMenu {
        let elements: [UIMenuElement] = actions.compactMap { action in
            switch action {
            // Call actions are surfaced as dedicated bar buttons, not menu items.
            case .audioCall,
                 .videoCall:
                nil
            case .leaveChat:
                UIAction(
                    title: String(localized: .leaveChatAction),
                    image: UIImage(resource: .icon20LogOut),
                    attributes: .destructive
                ) { [weak self, weak view, weak delegate] _ in
                    guard let delegate else { return }
                    self?.showLeaveChatConfirmation(from: view, chatMetadata: chatMetadata, delegate: delegate)
                }
            case .blockUser:
                UIAction(
                    title: String(localized: .blockUserAction),
                    image: UIImage(resource: .iconBlock),
                    attributes: .destructive
                ) { [weak self, weak view, weak delegate] _ in
                    guard let delegate else { return }
                    self?.showBlockUserConfirmation(from: view, chatMetadata: chatMetadata, delegate: delegate)
                }
            case let .custom(action):
                UIDeferredMenuElement.uncached { completion in
                    let item = UIAction(title: action.titleProvider(), image: action.image) { _ in
                        action.handler()
                    }
                    completion([item])
                }
            }
        }

        return UIMenu(children: elements)
    }

    func showEditHistory(
        from view: ControllerBackedProtocol?,
        messageId: String
    ) {
        let editHistoryView = EditHistoryViewFactory.createView(messageId: messageId)
        view?.controller.present(editHistoryView, animated: true)
    }

    func dismissChat(from view: ControllerBackedProtocol?) {
        view?.controller.navigationController?.popViewController(animated: true)
    }

    func showDocument(at url: URL) {
        documentAdapter.previewDocument(at: url) { [logger] result in
            switch result {
            case .success:
                break
            case .failure:
                logger.error("Failed to show document at \(url)")
            }
        }
    }

    func showImagePicker(
        from view: ControllerBackedProtocol?,
        onAttachmentSelected: @escaping (([ChatAttachmentProviding]) -> Void)
    ) {
        self.onAttachmentSelected = onAttachmentSelected
        presentPicker(from: view)
    }

    func showAttachmentSelection(
        from view: ControllerBackedProtocol?,
        providers: [ChatAttachmentProviding],
        onComplete: @escaping (ProcessedAttachmentResult) -> Void
    ) {
        guard
            let previewView = ChatAttachmentsViewFactory.createView(
                providers: providers,
                flowState: flowState,
                onComplete: onComplete
            ) else {
            return
        }

        let navigationController = AppNavigationController(rootViewController: previewView.controller)
        navigationController.barSettings = NavigationBarSettings(style: .defaultStyle, shouldSetCloseButton: false)
        navigationController.modalPresentationStyle = .fullScreen

        view?.controller.present(navigationController, animated: true)
    }

    @MainActor func showAttachmentPreview(
        from view: ControllerBackedProtocol?,
        attachment: Chat.LocalMessage.Content.Attachment
    ) {
        switch attachment {
        case let .remoteDownloadable(fileVariant):
            let fileUrl = downloadStore.fileURL(for: fileVariant.filename)

            guard let utType = UTType(mimeType: fileVariant.meta.mimeType) else {
                return
            }

            showMedia(url: fileUrl, utType: utType, from: view?.controller)
        case let .localUploadable(file):
            let fileUrl = uploadStore.fileURL(for: file.relativeLocalPath)

            guard let utType = UTType(mimeType: file.meta.mimeType) else {
                return
            }

            showMedia(url: fileUrl, utType: utType, from: view?.controller)
        }
    }
}

extension ChatWireframe: MediaPreviewPresentable {}

extension ChatWireframe: PHPickerViewControllerDelegate {
    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        let providers: [ChatAttachmentProviding] = results.compactMap { result in
            let hasImage = result.itemProvider.registeredContentTypes.contains { $0.conforms(to: .image) }

            if hasImage {
                return PHImageAttachmentProvider(itemProvider: result.itemProvider)
            }

            let hasVideo = result.itemProvider.registeredContentTypes.contains { $0.conforms(to: .movie) }

            if hasVideo {
                return PHVideoAttachmentProvider(itemProvider: result.itemProvider)
            }

            return nil
        }

        picker.dismiss(animated: true) { [weak self] in
            let closure = self?.onAttachmentSelected
            self?.onAttachmentSelected = nil
            closure?(providers)
        }
    }
}

private extension ChatWireframe {
    func presentPicker(from view: ControllerBackedProtocol?) {
        var configuration = PHPickerConfiguration()
        configuration.selectionLimit = 1
        configuration.preferredAssetRepresentationMode = .current
        configuration.filter = .any(of: [.images, .videos])

        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = self

        view?.controller.present(picker, animated: true)
    }

    func showLeaveChatConfirmation(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        delegate: ChatMoreActionsDelegate
    ) {
        guard case .person = chatMetadata.chatId else {
            return
        }

        let leaveChatView = LeaveChatViewFactory.createView(
            username: chatMetadata.peerMetadata.name,
            onDelete: {
                delegate.didConfirmLeaveChat()
            },
            onCancel: {}
        )

        view?.controller.present(leaveChatView, animated: true)
    }

    func showBlockUserConfirmation(
        from view: ControllerBackedProtocol?,
        chatMetadata: ChatMetadata,
        delegate: ChatMoreActionsDelegate
    ) {
        guard case .person = chatMetadata.chatId else {
            return
        }

        let blockUserView = BlockUserViewFactory.createView(
            username: chatMetadata.peerMetadata.name,
            onBlock: {
                delegate.didConfirmBlockUser()
            },
            onCancel: {}
        )

        view?.controller.present(blockUserView, animated: true)
    }
}
