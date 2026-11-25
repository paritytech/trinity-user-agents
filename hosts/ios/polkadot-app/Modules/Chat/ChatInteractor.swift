import Coinage
import UIKit
import os
import Operation_iOS
import SubstrateSdk
import Foundation_iOS

final class ChatInteractor {
    weak var presenter: ChatInteractorOutputProtocol?

    private let chatId: Chat.Id
    let engine: ChatEngineProtocol
    private let reactionRepository: ChatReactionRepositoryProtocol
    private let reclaimCoinsService: any ReclaimCoinsServicing
    private let logger: LoggerProtocol
    private let usernameStorage: UsernameStoring
    private let permissionsService: CallPermissionsServicing

    private var messagesTask: Task<Void, Never>?
    private var metadataTask: Task<Void, Never>?
    private var footerTask: Task<Void, Never>?
    /// The last open or close of the room. Each one waits for the one before it.
    var roomTask: Task<Void, Never>?
    let state = ChatInteractorState()
    private let feedPolicy = OSAllocatedUnfairLock(initialState: ChatFeedPolicy())

    private let notificationsCleaner: any PushNotificationsCleaning

    private weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?

    init(
        chatId: Chat.Id,
        engine: ChatEngineProtocol,
        reactionRepository: ChatReactionRepositoryProtocol,
        reclaimCoinsService: any ReclaimCoinsServicing,
        usernameStorage: UsernameStoring = UsernameStorage(),
        permissionsService: CallPermissionsServicing = CallPermissionsService(),
        logger: LoggerProtocol = Logger.shared,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        notificationsCleaner: any PushNotificationsCleaning
    ) {
        self.chatId = chatId
        self.engine = engine
        self.reactionRepository = reactionRepository
        self.reclaimCoinsService = reclaimCoinsService
        self.usernameStorage = usernameStorage
        self.permissionsService = permissionsService
        self.logger = logger
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        self.notificationsCleaner = notificationsCleaner
    }

    deinit {
        messagesTask?.cancel()
        metadataTask?.cancel()
        footerTask?.cancel()
    }
}

private extension ChatInteractor {
    func subscribeMetadata() {
        metadataTask = Task { [weak self, logger, engine, usernameStorage] in
            guard let currentUsername = usernameStorage.username?.value else {
                logger.error("Unexpected missing username")
                return
            }

            do {
                let metadataStream = engine.chatMetadataStream()
                var lastMetadata: MessageListMetadata?
                var messageStore: ChatRoomPolicy.MessageStore?
                for try await chatMetadata in metadataStream {
                    if let chatMetadata {
                        let isIncomingRequest = chatMetadata.peerMetadata.input == .incomingRequest
                        self?.feedPolicy.withLock { $0.isIncomingRequest = isIncomingRequest }
                        let metadata = MessageListMetadata(chatMetadata: chatMetadata, myUsername: currentUsername)
                        lastMetadata = metadata
                        await self?.presenter?.didReceive(metadata: metadata)

                        let store = chatMetadata.roomPolicy?.messageStore
                        if store != messageStore {
                            messageStore = store
                            await self?.refreshMessageStore()
                        }
                    } else if let lastMetadata {
                        await self?.handleChatRemoval(lastMetadata: lastMetadata)
                        break
                    }
                }
            } catch {
                logger.error("Peer metadata stream failed: \(error)")
            }

            logger.debug("Peer metadata stream ended")
        }
    }

    func subscribeFooter() {
        footerTask = Task { [weak self] in
            do {
                guard let footerSeq = try await self?.engine.footerStream() else {
                    return
                }

                for try await footer in footerSeq {
                    await self?.presenter?.didReceiveFooter(footer)
                }
            } catch {
                self?.logger.error("Unexpected footer error: \(error)")
            }
        }
    }

    /// The product set the configuration of the room after the room screen subscribed to its messages: the screen
    /// follows the messages of the new store.
    func refreshMessageStore() async {
        guard await engine.refreshMessageStore() else {
            return
        }

        messagesTask?.cancel()
        subscribeMessages()
    }

    func subscribeMessages() {
        messagesTask = Task { [weak self] in
            do {
                guard let snapshots = try await self?.engine.messageSnapshots() else {
                    return
                }

                for try await snapshot in snapshots {
                    let hadMessages = await self?.state.model?.allMessages.isEmpty == false

                    guard let model = await self?.state.onSnapshot(snapshot) else {
                        return
                    }

                    let isAllowed = self?.feedPolicy.withLock { $0.allows(model, hadMessages: hadMessages) } ?? true
                    guard isAllowed else { continue }

                    await self?.presenter?.didReceive(listModel: model)
                }
            } catch {
                self?.logger.error("Can't update due to error: \(error)")
            }
        }
    }

    func handleChatRemoval(lastMetadata: MessageListMetadata) async {
        feedPolicy.withLock { $0.isFrozen = true }
        messagesTask?.cancel()

        switch await engine.removalReason() {
        case .requestCancelled:
            await presenter?.didReceive(metadata: lastMetadata.withCancelledRequest())
        case .other:
            await presenter?.didRemoveChat()
        }
    }
}

extension ChatInteractor: ChatInteractorInputProtocol {
    func setup() {
        subscribeMetadata()
        subscribeMessages()
        subscribeFooter()
    }

    func isMicrophoneDenied() -> Bool {
        permissionsService.isMicrophoneDenied
    }

    func send(
        text: String?,
        attachments: [ProcessedAttachment]?,
        replyToMessageId: String?
    ) {
        Task {
            do {
                let content: Chat.LocalMessage.Content
                // map empty "" messages to nil
                let text = text.flatMap { $0.isEmpty ? nil : $0 }

                // TODO: Add support for attachments in reply and edit in the separate task
                if let replyToMessageId {
                    let richTextContent = ChatRemoteMessageContent.RichText(
                        text: text,
                        attachments: nil
                    )
                    let replyContent = Chat.RemoteMessageContentV1.MessageContent.ReplyContent(
                        messageId: replyToMessageId,
                        ownContent: richTextContent
                    )
                    content = .reply(replyContent)
                } else {
                    let messageAttachments = attachments?.map { $0.toMessageAttachment() }

                    if let messageAttachments, !messageAttachments.isEmpty {
                        let richTextContent = Chat.LocalMessage.Content.RichText(
                            text: text,
                            attachments: messageAttachments
                        )

                        content = .richText(richTextContent)
                    } else {
                        content = .text(text ?? "")
                    }
                }

                try await self.engine.sendUserMessage(with: content)
            } catch {
                self.logger.error("\(error)")
            }
        }
    }

    func notifyViewAppeared() {
        updateForegroundVisibility(isVisible: true)
    }

    func notifyViewDisappeared() {
        updateForegroundVisibility(isVisible: false)
    }

    func readAllBefore(identifier: Chat.MessageId) {
        readMessages([identifier])
    }

    func readMessages(_ identifiers: [Chat.MessageId]) {
        Task { [engine, notificationsCleaner] in
            do {
                // A product-source room knows which message the user saw last, and tells the product.
                if await engine.messageStore() == .product {
                    let seesNewMessage = await self.state.noteSeen(messageIds: identifiers)
                    try await engine.markAsSeen(messageIds: Set(identifiers))
                    if seesNewMessage {
                        try await notificationsCleaner.cleanNotifications(for: chatId, messageIds: identifiers)
                    }
                    return
                }

                let messageIdsToMark = await self.state.markAsSeen(messageIds: identifiers)

                guard !messageIdsToMark.isEmpty else {
                    return
                }
                try await engine.markAsSeen(messageIds: messageIdsToMark)
                self.logger.debug("Marked as seen: \(messageIdsToMark.count) messages")

                try await notificationsCleaner.cleanNotifications(
                    for: chatId,
                    messageIds: Array(messageIdsToMark)
                )
                self.logger.debug("Removed push notifications")
            } catch {
                self.logger.error("Can't mark due to error: \(error)")
            }
        }
    }

    func toggleReaction(messageId: String, emoji: String) {
        Task {
            do {
                if await engine.messageStore() == .product {
                    try await perform(.toggleReaction(messageId: messageId, emoji: emoji))
                    return
                }

                let userOrigin = Chat.LocalMessage.Origin.user

                let ownReactions = try await reactionRepository
                    .fetchReactions(for: messageId)
                    .asyncExecute()
                    .filter { userOrigin == $0.origin }

                let hasReaction = ownReactions.contains(where: {
                    $0.emoji == emoji
                })

                if hasReaction {
                    try await removeOwnReactions(ownReactions)
                } else {
                    try await updateReactions(
                        from: ownReactions,
                        to: .init(
                            messageId: messageId,
                            emoji: emoji,
                            origin: userOrigin,
                            chatId: chatId,
                            timestamp: Date().toChatTimestamp()
                        )
                    )
                }
            } catch {
                logger.error("\(error)")
            }
        }
    }

    private func updateReactions(
        from ownReactions: [Chat.MessageReaction],
        to reaction: Chat.MessageReaction
    ) async throws {
        try await reactionRepository
            .updateReaction(
                reaction,
                removing: ownReactions
            )
            .asyncExecute()

        try await engine.sendUserMessage(with: .reacted(.init(
            messageId: reaction.messageId,
            emoji: reaction.emoji
        )))

        for reaction in ownReactions {
            try await engine.sendUserMessage(
                with: .reactionRemoved(.init(
                    messageId: reaction.messageId,
                    emoji: reaction.emoji
                ))
            )
        }
    }

    private func removeOwnReactions(
        _ ownReactions: [Chat.MessageReaction]
    ) async throws {
        try await reactionRepository
            .removeReactions(ownReactions)
            .asyncExecute()

        for reaction in ownReactions {
            try await engine.sendUserMessage(
                with: .reactionRemoved(.init(
                    messageId: reaction.messageId,
                    emoji: reaction.emoji
                ))
            )
        }
    }

    func sendEdit(messageId: String, newText: String) {
        Task {
            do {
                let editedContent = Chat.RemoteMessageContentV1.MessageContent.EditedContent(
                    messageId: messageId,
                    newContent: .init(text: newText, attachments: nil)
                )

                // Send to peer
                try await engine.sendUserMessage(with: .edited(editedContent))
            } catch {
                logger.error("\(error)")
            }
        }
    }

    func deleteMessage(messageId: String) {
        Task {
            do {
                try await perform(.delete(messageId: messageId))
            } catch {
                logger.error("\(error)")
            }
        }
    }

    func leaveChat() {
        Task {
            do {
                try await engine.leaveChat()
                await presenter?.didLeftChat()
            } catch {
                logger.error("Unexpected error: \(error)")
            }
        }
    }

    func blockUser() {
        Task {
            do {
                try await engine.blockUser()
                await presenter?.didBlockUser()
            } catch {
                logger.error("Unexpected error while blocking user: \(error)")
            }
        }
    }

    func unblockUser() {
        Task {
            do {
                try await engine.unblockUser()
            } catch {
                logger.error("Unexpected error while unblocking user: \(error)")
            }
        }
    }

    func acceptChatRequest() {
        Task {
            do {
                try await engine.acceptChatRequest()
            } catch {
                logger.error("Unexpect error while accepting request: \(error)")
            }
        }
    }

    func declineChatRequest() {
        Task {
            do {
                try await engine.declineChatRequest()
                await presenter?.didDeclineChatRequest()
            } catch {
                logger.error("Unexpect error while declining request: \(error)")
            }
        }
    }

    func cancelChatRequest() {
        Task { [engine, logger] in
            do {
                try await engine.cancelChatRequest()
            } catch ChatRequestCancelError.notPending {
                logger.debug("Chat request is no longer pending")
            } catch {
                logger.error("Unexpected error while cancelling request: \(error)")
            }
        }
    }

    func processAction(_ action: Chat.Action) {
        Task {
            await engine.processAction(action)
        }
    }

    func reclaimTransfer(messageId: Chat.MessageId, coinKeys: [Data]) {
        Task {
            do {
                try await reclaimCoinsService.reclaim(
                    coinKeys: coinKeys,
                    groupId: ChatTransferGroups.reclaim(messageId)
                )
            } catch {
                logger.error("Failed to reclaim transfer \(messageId): \(error)")
                await presenter?.didFailReclaim(messageId: messageId, error: error)
            }
        }
    }
}

private extension ChatInteractor {
    /// Sends the action to the message source. A refusal is logged.
    func perform(_ action: ChatSourceAction) async throws {
        if case let .refused(reason) = try await engine.perform(action) {
            logger.error("The product refused \(action): \(reason ?? "")")
        }
    }
}

extension ChatInteractor {
    func updateForegroundVisibility(isVisible: Bool) {
        guard let foregroundVisibilityReporter else {
            return
        }

        guard isVisible else {
            foregroundVisibilityReporter.updateVisibleScreen(.other)
            return
        }

        switch chatId {
        case let .person(accountId):
            foregroundVisibilityReporter.updateVisibleScreen(.chat(accountId: accountId))
        case let .chatExtension(chatExtensionId, _):
            foregroundVisibilityReporter.updateVisibleScreen(.chatExtension(extensionId: chatExtensionId))
        }
    }
}

private struct ChatFeedPolicy {
    var isFrozen = false
    var isIncomingRequest = false

    // An incoming request always holds its request message, so an empty feed means deletion.
    func allows(_ model: MessageListModel, hadMessages: Bool) -> Bool {
        guard !isFrozen else { return false }

        return !(isIncomingRequest && hadMessages && model.allMessages.isEmpty)
    }
}
