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
    /// Picks the message source again when the product changed where the messages of the room live since the
    /// source was picked. Gives true when the source changed, so the screen subscribes to the new one.
    func refreshMessageStore() async -> Bool
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

    func refreshMessageStore() async -> Bool {
        false
    }
}

final class ChatEngine {
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let sourceContext: ChatRoomSourceContext
    let chatProvider: ChatContactDataProviderMaking
    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    let chatExtensionRegistry: ChatExtensionsRegistering
    let settings: SettingsManagerProtocol & ChatExtensionBotSettings
    let chatId: Chat.Id
    let logger: LoggerProtocol

    init(
        chatId: Chat.Id,
        chatExtensionRegistry: ChatExtensionsRegistering,
        messageProviderFactory: ChatMessageDataProviderMaking,
        chatProvider: ChatContactDataProviderMaking,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        settings: SettingsManagerProtocol & ChatExtensionBotSettings,
        productRoomServices: ProductRoomServices? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatId = chatId
        self.chatExtensionRegistry = chatExtensionRegistry
        self.settings = settings
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
        try await sourceContext.source().perform(action)
    }

    func openRoom() async {
        await sourceContext.open()
    }

    func closeRoom() async {
        await sourceContext.close()
    }

    func refreshMessageStore() async -> Bool {
        await sourceContext.refresh()
    }

    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws {
        try await performRefusable(.send(content))
    }

    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws {
        _ = try await sourceContext.source().perform(.markSeen(messageIds))
    }

    func chatMetadataStream() -> AnyAsyncSequence<ChatMetadata?> {
        chatProvider
            .subscribeChat(by: chatId)
            .map { [chatExtensionRegistry] optChat in
                optChat?.chatMetadata(using: chatExtensionRegistry)
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

    // Contact actions belong to the contact rooms of the chat product; any other room refuses them.

    func leaveChat() async throws {
        try await performRefusable(.contactAction(.leave))
    }

    func blockUser() async throws {
        try await performRefusable(.contactAction(.block))
    }

    func unblockUser() async throws {
        try await performRefusable(.contactAction(.unblock))
    }

    func acceptChatRequest() async throws {
        try await performRefusable(.contactAction(.acceptRequest))
    }

    func declineChatRequest() async throws {
        try await performRefusable(.contactAction(.declineRequest))
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
    /// Throws ``ChatUserActionRefusal`` when the product refuses the action.
    func performRefusable(_ action: ChatSourceAction) async throws {
        if case let .refused(reason) = try await sourceContext.source().perform(action) {
            throw ChatUserActionRefusal(reason: reason)
        }
    }
}
