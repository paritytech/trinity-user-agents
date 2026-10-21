import Foundation
import Operation_iOS
import AsyncExtensions
import AsyncAlgorithms
import PolkadotUI
import Keystore_iOS
import Foundation_iOS

protocol ChatEngineProtocol {
    func subscribe() async throws -> AsyncExtensions.AnyAsyncSequence<[Chat.LocalMessage]>
    /// The window of messages with its paging state. The default wraps `subscribe()` as a complete window.
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
    /// Replaces a message with the deleted tombstone and tells the extension of a product room.
    func deleteMessage(_ messageId: Chat.MessageId) async throws
    /// Where the messages of the room live. The default is the host database.
    func messageStore() async -> ChatRoomPolicy.MessageStore
    /// Asks for the page before the window. The default window holds the whole room.
    func loadOlder() async -> ChatPageOutcome
    /// Asks for the page after the window. The default window holds the whole room.
    func loadNewer() async -> ChatPageOutcome
    /// Asks for pages until the message is in the window. Gives false when the message is not in the room.
    func jump(to messageId: Chat.MessageId) async -> Bool
    /// Writes an action of the user to the message source of the room.
    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome
    /// The room screen shows the room. A product room holds its product until `closeRoom`.
    func openRoom() async
    func closeRoom() async
    /// Opens the room again and asks for the newest page, after the room became unavailable.
    func retryRoom() async
}

extension ChatEngineProtocol {
    func messageSnapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        try await subscribe()
            .map { MessageListSnapshot(messages: $0) }
            .eraseToAnyAsyncSequence()
    }

    func messageStore() async -> ChatRoomPolicy.MessageStore {
        .host
    }

    func loadOlder() async -> ChatPageOutcome {
        .end
    }

    func loadNewer() async -> ChatPageOutcome {
        .end
    }

    func jump(to _: Chat.MessageId) async -> Bool {
        false
    }

    func perform(_: ChatSourceAction) async throws -> ChatSourceActionOutcome {
        throw ChatMessageSourceError.unsupported
    }

    func openRoom() async {}

    func closeRoom() async {}

    func retryRoom() async {}
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
    /// The outbox of the attachments that the host stages (P10.3). It owns Retry and Delete of their messages.
    let stager: ProductAttachmentStager
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
        stager: ProductAttachmentStager = .shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatId = chatId
        self.stager = stager
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

        let messageDeletionRepository = AnyDataProviderRepository(
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

        let coreDataSource = CoreDataMessageSource(
            chatId: chatId,
            chatExtensionRegistry: chatExtensionRegistry,
            messageProviderFactory: messageProviderFactory,
            historyRepository: historyRepository,
            messageRepository: messageRepository,
            messageStatusUpdateRepository: messageStatusUpdateRepository,
            messageDeletionRepository: messageDeletionRepository,
            attachmentsUpdateRepository: attachmentsUpdateRepository,
            logger: logger
        )

        sourceContext = ChatRoomSourceContext(
            chatId: chatId,
            coreDataSource: coreDataSource,
            services: productRoomServices,
            policy: { [chatRepository] in
                let chat = try? await chatRepository
                    .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
                    .asyncExecute()
                return chat?.roomPolicy
            },
            logger: logger
        )
    }
}

extension ChatEngine: ChatEngineProtocol {
    func subscribe() async throws -> AsyncExtensions.AnyAsyncSequence<[Chat.LocalMessage]> {
        try await messageSnapshots()
            .map(\.messages)
            .eraseToAnyAsyncSequence()
    }

    func messageSnapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        try await sourceContext.source().snapshots()
    }

    func messageStore() async -> ChatRoomPolicy.MessageStore {
        await sourceContext.messageStore()
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
        // A failed attachment of the outbox stages again; the product never had its message.
        if case let .retry(messageId) = action, chatId.roomId != nil, await stager.retry(messageId) {
            return .done
        }
        return try await sourceContext.source().perform(action)
    }

    func openRoom() async {
        await sourceContext.open()
    }

    func closeRoom() async {
        await sourceContext.close()
    }

    func retryRoom() async {
        await sourceContext.retry()
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

        // A product-source room reports `Seen` through `user_action` from its source (02-DESIGN §13 item 11).
        if await messageStore() == .host {
            await reportSeen(messageIds)
        }
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
            messageRepository: messageRepository,
            chatId: chatId
        )
        let extensions = chatExtensionRegistry.getExtensions(for: chatId)
        for chatExtension in extensions {
            await chatExtension.process(action: action, context: actionContext)
        }
    }

    func deleteMessage(_ messageId: Chat.MessageId) async throws {
        // An attachment of the outbox goes with its bytes. The product never had its message, so it is not told.
        if chatId.roomId != nil, await stager.delete(messageId) {
            return
        }

        try await performRefusable(.delete(messageId))

        // A product-source room tells its product through `user_action` (02-DESIGN §13 item 11).
        guard await messageStore() == .host, case let .chatExtension(extensionId, roomId) = chatId else {
            return
        }

        let handler = chatExtensionRegistry.getChatExtensionBot(for: extensionId) as? ChatUserDeletionHandling
        await handler?.userDeleted(messageId: messageId, roomId: roomId)
    }
}

private extension ChatEngine {
    func sendUserMessageToExistingChat(with content: Chat.LocalMessage.Content) async throws {
        try await performRefusable(.send(content))
    }

    /// Throws ``ChatUserActionRefusal`` when the product refuses the action, so the room screen shows why.
    func performRefusable(_ action: ChatSourceAction) async throws {
        if case let .refused(reason) = try await sourceContext.source().perform(action) {
            throw ChatUserActionRefusal(reason: reason)
        }
    }
}
