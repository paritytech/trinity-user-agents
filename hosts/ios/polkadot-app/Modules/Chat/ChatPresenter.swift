import Foundation
import Keystore_iOS
import PolkadotUI
import Products
import SubstrateSdk
import UIKit.UIPasteboard

final class ChatPresenter {
    weak var view: ChatViewProtocol?
    let wireframe: ChatWireframeProtocol
    let interactor: ChatInteractorInputProtocol
    let viewModelFactory: ChatViewModelMaking
    let moduleNavigator: ModuleNavigating
    let paymentAssetViewModelFactory: PaymentAssetViewModelMaking

    private var listModel: MessageListModel?
    private var metadata: MessageListMetadata?
    private var footer: (any HashableContentConfiguration)?
    private var paymentAssetViewModel: PaymentAssetViewModelProtocol?

    init(
        interactor: ChatInteractorInputProtocol,
        wireframe: ChatWireframeProtocol,
        viewModelFactory: ChatViewModelMaking,
        moduleNavigator: ModuleNavigating,
        paymentAssetViewModelFactory: PaymentAssetViewModelMaking = PaymentAssetViewModelFactory()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.moduleNavigator = moduleNavigator
        self.paymentAssetViewModelFactory = paymentAssetViewModelFactory
    }

    deinit {
        paymentAssetViewModel?.cancel()
    }
}

// MARK: ChatPresenterProtocol

extension ChatPresenter: ChatPresenterProtocol {
    var chatId: Chat.Id? {
        metadata?.chatMetadata.chatId
    }

    func setup() {
        interactor.setup()
        subscribeToPaymentAsset()
    }

    func viewWillAppear() {
        interactor.notifyViewAppeared()
    }

    func viewWillDisappear() {
        interactor.notifyViewDisappeared()
    }

    func send(text: String, replyToMessageId: String? = nil) {
        interactor.send(
            text: text,
            attachments: nil,
            replyToMessageId: replyToMessageId
        )
    }

    func sendEdit(messageId: String, newText: String) {
        interactor.sendEdit(messageId: messageId, newText: newText)
    }

    func makeTransfer() {
        guard let chatMetadata = metadata?.chatMetadata else {
            return
        }

        wireframe.showSendAsset(from: view, chatMetadata: chatMetadata)
    }

    func readTillMessage(identifier: String) {
        interactor.readAllBefore(identifier: identifier)
    }

    func startCall(_ callType: ChatCallType) {
        handleStartCall(callType: callType)
    }

    func onReply(for messageId: String) {
        handleReplyAction(for: messageId)
    }

    func onScrollToBottom() {
        guard let messages = listModel?.allMessages.map(\.messageId) else {
            return
        }

        interactor.readMessages(messages)
    }

    func showAttachmentSelection() {
        wireframe.showImagePicker(from: view) { [weak self] providers in
            guard !providers.isEmpty else {
                return
            }

            self?.confirmSelectedAttachment(providers)
        }
    }

    func onScrollToReaction() {
        guard let targetMessageId = listModel?.oldestNewReactionTargetMessageId else {
            return
        }

        interactor.readMessages([targetMessageId])
    }
}

extension ChatPresenter: ChatInteractorOutputProtocol {
    func didReceive(metadata: MessageListMetadata) {
        self.metadata = metadata

        provideViewModel()
        provideMoreActions()
    }

    func didReceive(listModel: MessageListModel) {
        self.listModel = listModel

        markInvisibleMessagesAsRead(in: listModel)
        provideViewModel()
    }

    func didReceiveFooter(_ footer: (any HashableContentConfiguration)?) {
        self.footer = footer

        if viewModelProvidedOnce {
            provideFooter()
        } else {
            provideViewModel()
        }
    }

    func didLeftChat() {
        wireframe.dismissChat(from: view)
    }

    func didBlockUser() {
        wireframe.dismissChat(from: view)
    }

    func didDeclineChatRequest() {
        wireframe.dismissChat(from: view)
    }
}

private extension ChatPresenter {
    var viewModelProvidedOnce: Bool {
        listModel != nil && metadata != nil
    }

    func subscribeToPaymentAsset() {
        let viewModel = paymentAssetViewModelFactory.makeViewModel()
        paymentAssetViewModel = viewModel

        viewModel.bind { [weak self] _ in
            self?.provideViewModel()
        }
    }

    func provideViewModel() {
        guard let listModel, let metadata else {
            return
        }

        let contextActions = ChatViewModelActions(
            reply: { [weak self] messageId in
                self?.handleReplyAction(for: messageId)
            },
            copy: { [weak self] content in
                self?.handleCopyAction(content: content)
            },
            edit: { [weak self] messageId in
                self?.handleEditAction(for: messageId)
            },
            toggleReaction: { [weak self] messageId, emoji in
                self?.interactor.toggleReaction(messageId: messageId, emoji: emoji)
            },
            showReactionDetails: { [weak self] messageId in
                self?.handleShowReactionDetails(for: messageId)
            },
            acceptChatRequest: { [weak self] in
                self?.interactor.acceptChatRequest()
            },
            declineChatRequest: { [weak self] in
                self?.interactor.declineChatRequest()
            },
            showEditHistory: { [weak self] messageId in
                self?.handleShowEditHistory(for: messageId)
            },
            showFile: { [weak self] url in
                self?.handleShowFile(at: url)
            },
            processAction: { [weak self] action in
                self?.interactor.processAction(action)
            },
            selectAttachment: { [weak self] attachment in
                MainActor.assumeIsolated {
                    self?.wireframe.showAttachmentPreview(from: self?.view, attachment: attachment)
                }
            },
            unblockUser: { [weak self] in
                self?.interactor.unblockUser()
            },
            startCall: { [weak self] callType in
                self?.handleStartCall(callType: callType)
            },
            openProduct: { [moduleNavigator] productPage in
                Task {
                    @MainActor in moduleNavigator.openProduct(page: productPage)
                }
            }
        )

        let viewModel = viewModelFactory.viewModel(
            model: listModel,
            metadata: metadata,
            actions: contextActions,
            footerConfiguration: footer
        )
        view?.didReceive(viewModel: viewModel)
    }

    func provideMoreActions() {
        guard let metadata else {
            return
        }

        let actions = metadata.peerMetadata.moreActions

        let callActions: [ChatCallType] = actions.compactMap { action in
            switch action {
            case .audioCall: .audio
            case .videoCall: .video
            default: nil
            }
        }
        view?.didReceive(callActions: callActions)

        let menuActions = actions.filter { action in
            switch action {
            case .audioCall,
                 .videoCall: false
            default: true
            }
        }
        let menu = menuActions.isEmpty ? nil : MainActor.assumeIsolated {
            wireframe.makeContactActionsMenu(
                from: view,
                chatMetadata: metadata.chatMetadata,
                actions: menuActions,
                delegate: self
            )
        }
        view?.didReceive(contactMenu: menu)
    }

    func provideFooter() {
        view?.didReceive(footer: footer)
    }

    func handleReplyAction(for messageId: String) {
        guard
            let listModel,
            let message = listModel.getMessage(by: messageId),
            let metadata
        else {
            return
        }

        let username: String = message.status.isIncoming ? metadata.peerName : String(localized: .chatReplyYou)

        let currentText: String =
            if let latestEditedText = listModel.getLatestEdit(for: messageId)?.newText {
                latestEditedText
            } else {
                switch message.content {
                case let .text(content):
                    content
                case let .staticTextImageContent(content):
                    content.text ?? String(localized: .chatReplyImage)
                case let .reply(replyContent):
                    replyContent.ownContent.text ?? String(localized: .chatReplyPlaceholder)
                case let .send(content):
                    // Legacy
                    viewModelFactory.transferPreviewText(
                        content: .init(totalValue: content.amount, coinKeys: []),
                        isIncoming: message.status.isIncoming,
                        peerName: metadata.peerName
                    )
                case let .coinageSend(content):
                    viewModelFactory.transferPreviewText(
                        content: content,
                        isIncoming: message.status.isIncoming,
                        peerName: metadata.peerName
                    )
                case let .chatRequest(content):
                    content.welcomeMessage?.text ?? String(localized: .chatReplyPlaceholder)
                case let .versionedChatRequest(content):
                    content.ensureV1().welcomeMessage?.text ?? String(localized: .chatReplyPlaceholder)
                default:
                    String(localized: .chatReplyPlaceholder)
                }
            }

        view?.showReply(messageId: messageId, username: username, text: currentText)
    }

    func handleCopyAction(content: String) {
        UIPasteboard.general.string = content
    }

    func handleEditAction(for messageId: String) {
        guard
            let listModel,
            let message = listModel.getMessage(by: messageId)
        else {
            return
        }

        // Only allow editing own messages
        guard !message.status.isIncoming else {
            return
        }

        let currentText: String
        if let latestEditedText = listModel.getLatestEdit(for: messageId)?.newText {
            currentText = latestEditedText
        } else {
            switch message.content {
            case let .text(text):
                currentText = text
            case let .richText(richText):
                currentText = richText.text ?? ""
            case let .reply(replyContent):
                currentText = replyContent.ownContent.text ?? ""
            default:
                return
            }
        }

        view?.showEdit(messageId: messageId, currentText: currentText)
    }

    func handleShowReactionDetails(for messageId: String) {
        guard
            let aggregates = listModel?.reactionsByMessageId[messageId],
            !aggregates.isEmpty,
            let metadata else {
            return
        }

        let reactionGroups = aggregates.map { aggregate in
            let reactors = aggregate.reactors.map { reactor in
                let username: String =
                    switch reactor.origin {
                    case .user:
                        metadata.myUsername
                    case .contact,
                         .chatExtension:
                        metadata.peerName
                    }
                let date = Date.fromChatTimestamp(reactor.timestamp)
                return ReactionDetailsViewModel.Reactor(
                    id: "\(reactor.timestamp)",
                    username: username,
                    timestamp: date
                )
            }
            return ReactionDetailsViewModel.ReactionGroup(
                emoji: aggregate.emoji,
                count: aggregate.count,
                reactors: reactors
            )
        }

        let totalCount = aggregates.reduce(0) { $0 + $1.count }
        let viewModel = ReactionDetailsViewModel(totalCount: totalCount, reactions: reactionGroups)
        view?.showReactionDetails(viewModel: viewModel)
    }

    func handleShowEditHistory(for messageId: String) {
        MainActor.assumeIsolated {
            wireframe.showEditHistory(
                from: view,
                messageId: messageId
            )
        }
    }

    func handleShowFile(at url: URL) {
        MainActor.assumeIsolated {
            wireframe.showDocument(at: url)
        }
    }

    func handleStartCall(callType: ChatCallType) {
        guard let metadata else { return }
        MainActor.assumeIsolated {
            if interactor.isMicrophoneDenied() {
                wireframe.presentMicrophoneAccessDenied(from: view)
                return
            }

            wireframe.showCall(
                from: view,
                chatMetadata: metadata.chatMetadata,
                callType: callType
            )
        }
    }

    func markInvisibleMessagesAsRead(in model: MessageListModel) {
        let invisibleMessageIds = model.allMessages.compactMap { message -> String? in
            guard
                case .incoming(.new) = message.status,
                !viewModelFactory.isMessageVisible(message)
            else {
                return nil
            }

            return message.messageId
        }

        guard !invisibleMessageIds.isEmpty else {
            return
        }

        interactor.readMessages(invisibleMessageIds)
    }

    func confirmSelectedAttachment(_ attachmentProviders: [ChatAttachmentProviding]) {
        MainActor.assumeIsolated {
            wireframe.showAttachmentSelection(
                from: view,
                providers: attachmentProviders
            ) { [weak self] result in
                self?.interactor.send(
                    text: result.message,
                    attachments: result.attachments,
                    replyToMessageId: nil
                )
            }
        }
    }
}

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
