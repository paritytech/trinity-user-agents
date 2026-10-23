import Foundation
import Keystore_iOS
import PolkadotUI
import Products
import SubstrateSdk
import TrUAPIHost
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
    /// The parts of a product room that the room screen asks its product for. Nil for a person chat.
    var productRoom: ChatProductRoomPresenter?
    /// The screen closed once already. A product deletes the room of a declined request or a left chat, which
    /// closes the screen a second time.
    private var isClosed = false

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
        interactor.openRoom()
        MainActor.assumeIsolated { productRoom?.screenWillAppear() }
    }

    func viewWillDisappear() {
        interactor.notifyViewDisappeared()
        interactor.closeRoom()
        MainActor.assumeIsolated { productRoom?.screenWillDisappear() }
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
        closeChat()
    }

    func didBlockUser() {
        closeChat()
    }

    func didDeclineChatRequest() {
        closeChat()
    }

    func didRemoveChat() {
        closeChat()
    }
}

private extension ChatPresenter {
    @MainActor
    func closeChat() {
        guard !isClosed else { return }

        isClosed = true
        wireframe.dismissChat(from: view)
    }

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

        var viewModel = viewModelFactory.viewModel(
            model: listModel,
            metadata: metadata,
            actions: contextActions,
            footerConfiguration: footer
        )
        viewModel.paging = ChatViewLayout.ViewModel.Paging(paging: listModel.paging)
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

        // A product room that offers reaction details keeps the time of each reaction. Without its answer, the
        // list shows the reactions that the room holds, as for any other room.
        if metadata.chatMetadata.roomPolicy?.offersReactionDetails == true,
           let productRoom = MainActor.assumeIsolated({ productRoom }) {
            Task { @MainActor [weak self] in
                let details = await productRoom.reactionDetails(of: messageId)
                self?.showReactionDetails(
                    aggregates: aggregates,
                    details: details.map { ($0, .chatExtension(productRoom.productId)) },
                    metadata: metadata
                )
            }
            return
        }

        showReactionDetails(aggregates: aggregates, details: nil, metadata: metadata)
    }

    func showReactionDetails(
        aggregates: [Chat.MessageReactionAggregate],
        details: (reactions: [ChatReactionDetail], contactOrigin: Chat.LocalMessage.Origin)?,
        metadata: MessageListMetadata
    ) {
        let reactionGroups = aggregates.map { aggregate in
            let reactors = details.map { aggregate.reactors(with: $0.reactions, contactOrigin: $0.contactOrigin) }
                ?? aggregate.reactors
            let reactorModels = reactors.map { reactor in
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
                reactors: reactorModels
            )
        }

        let totalCount = aggregates.reduce(0) { $0 + $1.count }
        let viewModel = ReactionDetailsViewModel(totalCount: totalCount, reactions: reactionGroups)
        view?.showReactionDetails(viewModel: viewModel)
    }

    func handleShowEditHistory(for messageId: String) {
        MainActor.assumeIsolated {
            // A room whose product keeps the messages asks the product for the history. Any other room reads its
            // own store.
            if metadata?.chatMetadata.roomPolicy?.messageStore == .product,
               let productRoom, let message = listModel?.getMessage(by: messageId),
               let wireframe = wireframe as? ChatWireframe {
                wireframe.showEditHistory(
                    from: view,
                    messageId: messageId,
                    interactor: productRoom.editHistoryInteractor(for: message)
                )
                return
            }

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
        guard metadata != nil else { return }
        MainActor.assumeIsolated {
            if interactor.isMicrophoneDenied() {
                wireframe.presentMicrophoneAccessDenied(from: view)
                return
            }

            // Only the rooms of the chat product place calls; any other room has no call to start.
            _ = routesCallToProduct(callType)
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
