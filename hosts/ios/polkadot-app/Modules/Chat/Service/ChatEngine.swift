import Foundation
import Operation_iOS
import AsyncExtensions
import AsyncAlgorithms
import PolkadotUI
import Keystore_iOS
import Foundation_iOS

protocol ChatEngineProtocol {
    func subscribe() async throws -> AsyncExtensions.AnyAsyncSequence<[Chat.LocalMessage]>
    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws
    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws
    func leaveChat() async throws
    func blockUser() async throws
    func unblockUser() async throws
    func acceptChatRequest() async throws
    func declineChatRequest() async throws
    func chatMetadataStream() -> AnyAsyncSequence<ChatMetadata?>
    func footerStream() async throws -> AnyAsyncSequence<(any HashableContentConfiguration)?>?
    func processAction(_ action: Chat.Action) async
    /// Gives true when the room's extension took the command, so the text must not be posted.
    func sendCommand(_ command: Chat.Command) async -> Bool
    /// Replaces a message with the deleted tombstone and tells the extension of a product room.
    func deleteMessage(_ messageId: Chat.MessageId) async throws
}

enum ChatEngineError: Error {
    case missingChat
}

final class ChatEngine {
    let historyRepository: AnyDataProviderRepository<ChatExtension.ProcessingHistory>
    let messageProviderFactory: ChatMessageDataProviderMaking
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let messageStatusUpdateRepository: AnyDataProviderRepository<Chat.ChatMessageStatusUpdate>
    let messageDeletionRepository: AnyDataProviderRepository<Chat.MessageDeletion>
    let attachmentsUpdateRepository: AnyDataProviderRepository<Chat.AttachmentUploadingUpdate>
    let chatProvider: ChatContactDataProviderMaking
    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    let chatExtensionRegistry: ChatExtensionsRegistering
    let settings: SettingsManagerProtocol & ChatExtensionBotSettings
    let chatId: Chat.Id
    let chatRequestContext: ChatRequestEngineContext
    let leaveChatService: LeaveChatServicing
    let blockUserService: BlockUserServicing
    let logger: LoggerProtocol

    init(
        chatId: Chat.Id,
        chatRequestContext: ChatRequestEngineContext,
        chatExtensionRegistry: ChatExtensionsRegistering,
        messageProviderFactory: ChatMessageDataProviderMaking,
        chatProvider: ChatContactDataProviderMaking,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        settings: SettingsManagerProtocol & ChatExtensionBotSettings,
        leaveChatService: LeaveChatServicing,
        blockUserService: BlockUserServicing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatId = chatId
        self.chatRequestContext = chatRequestContext
        self.chatExtensionRegistry = chatExtensionRegistry
        self.messageProviderFactory = messageProviderFactory
        self.settings = settings
        self.leaveChatService = leaveChatService
        self.blockUserService = blockUserService
        self.chatProvider = chatProvider
        self.logger = logger

        historyRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: .chatExtensionHistoryForChat(chatId),
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatExtensionProcessingHistoryMapper())
            )
        )

        messageRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: .localMessages(from: chatId),
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )

        attachmentsUpdateRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(AttachmentUploadingMapper())
            )
        )

        messageStatusUpdateRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageStatusUpdateMapper())
            )
        )

        messageDeletionRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageDeletionMapper())
            )
        )

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )
    }
}

extension ChatEngine: ChatEngineProtocol {
    func subscribe() async throws -> AsyncExtensions.AnyAsyncSequence<[Chat.LocalMessage]> {
        let chatProcessingHistory = try await historyRepository.fetchAllOperation(
            with: RepositoryFetchOptions()
        )
        .asyncExecute()

        let processingContext = ChatExtensionProcessingContext(
            chatId: chatId,
            initialHistory: chatProcessingHistory,
            messageRepository: messageRepository,
            attachmentsUpdateRepository: attachmentsUpdateRepository,
            processingHistoryRepository: historyRepository,
            logger: logger
        )

        let messageSequence = messageProviderFactory
            .subscribeChatMessages(chatId)
            .handleEvents(onElement: { messages in
                let extensions = self.chatExtensionRegistry.getExtensions(for: self.chatId)

                for message in messages {
                    await processingContext.processOncePerSession(message) { message in
                        for chatExtension in extensions {
                            let isProcessed = await processingContext.hasProcessed(
                                messageId: message.messageId,
                                extensionId: chatExtension.identifier
                            )

                            let result = await chatExtension.process(
                                message: message,
                                lastProcessingOutcome: isProcessed ? .previouslyProcessed : .firstEncounter,
                                context: processingContext
                            )

                            await processingContext.noteProcessingResult(
                                messageId: message.messageId,
                                extensionId: chatExtension.identifier,
                                result: result
                            )
                        }
                    }
                }

                await processingContext.flushChatExtensionHistory()
            })

        return await combineLatest(
            messageSequence,
            processingContext.messageOverridesSequence
        )
        .map { messages, overrides in
            messages.map { overrides.applyOverrides(to: $0) }
        }
        .eraseToAnyAsyncSequence()
    }

    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws {
        if await chatRequestContext.hasPendingRequest() {
            try await chatRequestContext.submitPendingOutgoingRequest(with: content)
        } else {
            try await sendUserMessageToExistingChat(with: content)
        }
    }

    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws {
        let updates = messageIds.map { Chat.ChatMessageStatusUpdate(messageId: $0, status: .incoming(.seen)) }

        guard !updates.isEmpty else {
            return
        }

        try await messageStatusUpdateRepository.saveOperation({ updates }, { [] }).asyncExecute()
        await reportSeen(messageIds)
    }

    func chatMetadataStream() -> AnyAsyncSequence<ChatMetadata?> {
        chatProvider
            .subscribeChat(by: chatId)
            .map { [weak self, chatExtensionRegistry] optChat in
                if let chat = optChat {
                    await self?.chatRequestContext.resetPendingOutgoingRequest()

                    return chat.chatMetadata(using: chatExtensionRegistry)
                } else {
                    return await self?.chatRequestContext.chatMetadataForPendingRequest()
                }
            }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }

    func footerStream() async throws -> AnyAsyncSequence<(any HashableContentConfiguration)?>? {
        switch chatId {
        case .person:
            return nil
        case let .chatExtension(extId, _):
            let chatBot = chatExtensionRegistry.getChatExtensionBot(for: extId)
                .flatMap { $0 as? ChatExtensionActionProvidable }
            guard let chatBot else { return nil }

            return try await chatBot.contentConfiguration()
        }
    }

    func leaveChat() async throws {
        let chat = try await chatRepository
            .fetchOperation(
                by: { [chatId] in chatId.rawRepresentation },
                options: RepositoryFetchOptions()
            )
            .asyncExecute()
            .mapOrThrow(ChatEngineError.missingChat)

        try await leaveChatService.leaveChat(chat)
    }

    func blockUser() async throws {
        guard case let .person(accountId) = chatId else {
            throw ChatEngineError.missingChat
        }

        try await blockUserService.blockUser(accountId: accountId)
    }

    func unblockUser() async throws {
        guard case let .person(accountId) = chatId else {
            throw ChatEngineError.missingChat
        }

        try await blockUserService.unblockUser(accountId: accountId)
    }

    func acceptChatRequest() async throws {
        try await chatRequestContext.acceptIncomingRequest(for: chatId)
    }

    func declineChatRequest() async throws {
        try await chatRequestContext.declineIncomingRequest(for: chatId)
    }

    func processAction(_ action: Chat.Action) async {
        let actionContext = ChatExtensionActionContext(
            messageRepository: messageRepository
        )
        let extensions = chatExtensionRegistry.getExtensions(for: chatId)
        for chatExtension in extensions {
            await chatExtension.process(action: action, context: actionContext)
        }
    }

    func sendCommand(_ command: Chat.Command) async -> Bool {
        guard case let .chatExtension(extensionId, roomId) = chatId else {
            return false
        }

        let handler = chatExtensionRegistry.getChatExtensionBot(for: extensionId) as? ChatCommandHandling
        return await handler?.handle(command: command, roomId: roomId) ?? false
    }

    func deleteMessage(_ messageId: Chat.MessageId) async throws {
        let deletion = Chat.MessageDeletion(messageId: messageId)
        try await messageDeletionRepository.saveOperation({ [deletion] }, { [] }).asyncExecute()

        guard case let .chatExtension(extensionId, roomId) = chatId else {
            return
        }

        let handler = chatExtensionRegistry.getChatExtensionBot(for: extensionId) as? ChatUserDeletionHandling
        await handler?.userDeleted(messageId: messageId, roomId: roomId)
    }
}

private extension ChatEngine {
    func sendUserMessageToExistingChat(with content: Chat.LocalMessage.Content) async throws {
        let status = determineInitialMessageStatus(chatId: chatId)
        let newMessage = Chat.LocalMessage
            .newMessage(to: chatId, content: content)
            .replacingStatus(status)

        try await messageRepository.saveOperation({ [newMessage] }, { [] }).asyncExecute()
    }

    func determineInitialMessageStatus(chatId: Chat.Id) -> Chat.LocalMessage.Status {
        switch chatId {
        case .chatExtension:
            .outgoing(.delivered)
        case .person:
            .outgoing(.new)
        }
    }
}
