import Foundation
import Operation_iOS
import AsyncExtensions
import AsyncAlgorithms
import PolkadotUI
import Keystore_iOS
import Foundation_iOS

protocol ChatEngineProtocol {
    /// The window of messages with its paging state.
    func messageSnapshots() async throws -> AnyAsyncSequence<MessageListSnapshot>
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
    /// Asks for the page before the window.
    func loadOlder() async -> ChatPageOutcome
    /// Asks for the page after the window.
    func loadNewer() async -> ChatPageOutcome
    /// Asks for pages until the message is in the window. Gives false when the message is not in the room.
    func jump(to messageId: Chat.MessageId) async -> Bool
    /// Writes an action of the user to the message source of the room.
    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome
    /// The room screen shows the room. A product room holds its product until `closeRoom`.
    func openRoom() async
    func closeRoom() async
}

enum ChatEngineError: Error {
    case missingChat
}

final class ChatEngine {
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let sourceContext: ChatRoomSourceContext
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
        productRoomServices: ProductRoomServices? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatId = chatId
        self.chatRequestContext = chatRequestContext
        self.chatExtensionRegistry = chatExtensionRegistry
        self.settings = settings
        self.leaveChatService = leaveChatService
        self.blockUserService = blockUserService
        self.chatProvider = chatProvider
        self.logger = logger

        let historyRepository = AnyDataProviderRepository(
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

        let attachmentsUpdateRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(AttachmentUploadingMapper())
            )
        )

        let messageStatusUpdateRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageStatusUpdateMapper())
            )
        )

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )

        let coreDataSource = CoreDataMessageSource(
            chatId: chatId,
            chatExtensionRegistry: chatExtensionRegistry,
            messageProviderFactory: messageProviderFactory,
            historyRepository: historyRepository,
            messageRepository: messageRepository,
            messageStatusUpdateRepository: messageStatusUpdateRepository,
            attachmentsUpdateRepository: attachmentsUpdateRepository,
            logger: logger
        )

        sourceContext = ChatRoomSourceContext(
            chatId: chatId,
            coreDataSource: coreDataSource,
            services: productRoomServices,
            logger: logger
        )
    }
}

extension ChatEngine: ChatEngineProtocol {
    func messageSnapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        try await sourceContext.source().snapshots()
    }

    func loadOlder() async -> ChatPageOutcome {
        await sourceContext.source().loadOlder()
    }

    func loadNewer() async -> ChatPageOutcome {
        await sourceContext.source().loadNewer()
    }

    func jump(to messageId: Chat.MessageId) async -> Bool {
        await sourceContext.source().jump(to: messageId)
    }

    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome {
        try await sourceContext.source().perform(action)
    }

    func openRoom() async {
        await sourceContext.open()
    }

    func closeRoom() async {
        await sourceContext.close()
    }

    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws {
        if await chatRequestContext.hasPendingRequest() {
            try await chatRequestContext.submitPendingOutgoingRequest(with: content)
        } else {
            try await sendUserMessageToExistingChat(with: content)
        }
    }

    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws {
        _ = try await sourceContext.source().perform(.markSeen(messageIds))
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
}

private extension ChatEngine {
    func sendUserMessageToExistingChat(with content: Chat.LocalMessage.Content) async throws {
        _ = try await sourceContext.source().perform(.send(content))
    }
}
