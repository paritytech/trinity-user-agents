import Foundation
import Keystore_iOS
import Operation_iOS
import AsyncExtensions
import AsyncAlgorithms
import Products

protocol ChatExtensionDiscoverContextProtocol {
    @discardableResult
    func sendNewMessage(
        from chatBot: ChatExtensionBotProtocol,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage

    @discardableResult
    func sendNewMessage(
        to chatBot: ChatExtensionBotProtocol,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage

    @discardableResult
    func sendNewMessage(
        from chatBot: ChatExtensionBotProtocol,
        roomId: String,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage

    func createRoom(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        name: String?,
        icon: String?
    ) async throws -> CreateRoomStatus

    func subscribeRooms(
        for chatBot: ChatExtensionBotProtocol
    ) async -> AnyAsyncSequence<[RoomInfo]>

    func setWelcomeMessages(
        from chatBot: ChatExtensionBotProtocol,
        with builder: () -> [Chat.LocalMessage.Content]
    ) async throws

    func getMessages(
        of type: Chat.LocalMessage.Content.ContentType
    ) async throws -> [Chat.LocalMessage]

    func getMessagesByContentKey(
        _ contentKey: String,
        with chatBot: ChatExtensionBotProtocol?
    ) async throws -> [Chat.LocalMessage]

    func modifyMessageContent(
        messageId: Chat.MessageId,
        content: Chat.LocalMessage.Content
    ) async throws

    func hasResponse(
        from extension: any ChatExtending,
        with id: ChatExtension.ActionId
    ) async throws -> Bool

    func addActionResponse(
        _ response: String,
        action: ChatExtension.ActionId,
        delayDelivery: MessageDeliveryDelay,
        chatExtension: ChatExtending
    ) async throws

    func hasDeliveredWelcomeMessages(for chatBot: ChatExtensionBotProtocol) async -> Bool

    func markWelcomeMessagesDelivered(for chatBot: ChatExtensionBotProtocol) async
}

actor ChatExtensionDiscoverContext {
    let settings: SettingsManagerProtocol & ChatExtensionBotSettings
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let storageFacade: StorageFacadeProtocol
    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    let roomMetadataRepository: AnyDataProviderRepository<Chat.RoomMetadataUpdate>
    let chatsProviderFactory: ChatContactDataProviderMaking

    init(
        settings: SettingsManagerProtocol & ChatExtensionBotSettings,
        storageFacade: StorageFacadeProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.settings = settings
        self.storageFacade = storageFacade

        messageRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )

        roomMetadataRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(ChatRoomMetadataUpdateMapper())
            )
        )

        chatsProviderFactory = ChatContactDataProviderFactory(
            repositoryFactory: ChatContactRepositoryFactory(storageFacade: storageFacade),
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

private extension ChatExtensionDiscoverContext {
    func createChatIfNeeded(with chatBot: ChatExtensionBotProtocol) async throws {
        guard !settings.hasWelcomeMessage(from: chatBot.identifier) else {
            // if welcome message is sent then chat should be there
            return
        }

        let chatId = Chat.Id.chatExtension(chatBot.identifier)

        let optChat = try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()

        guard optChat == nil else {
            return
        }

        let chat = Chat.LocalModel.newChatWithExtension(chatBot.identifier)
        try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()
    }
}

extension ChatExtensionDiscoverContext: ChatExtensionDiscoverContextProtocol {
    @discardableResult
    func sendNewMessage(
        from chatBot: ChatExtensionBotProtocol,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage {
        await messageDeliveryDelay.delay()

        try await createChatIfNeeded(with: chatBot)

        let message = Chat.LocalMessage.newExtensionMessage(
            chatBot.identifier,
            content: newContent
        )

        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        return message
    }

    @discardableResult
    func sendNewMessage(
        to chatBot: ChatExtensionBotProtocol,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage {
        await messageDeliveryDelay.delay()

        let newMessage = Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: .chatExtension(chatBot.identifier),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: Date().toChatTimestamp(),
            content: newContent,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        try await messageRepository.saveOperation({ [newMessage] }, { [] }).asyncExecute()

        return newMessage
    }

    func setWelcomeMessages(
        from chatBot: ChatExtensionBotProtocol,
        with builder: () -> [Chat.LocalMessage.Content]
    ) async throws {
        guard !settings.hasWelcomeMessage(from: chatBot.identifier) else {
            return
        }
        try await createChatIfNeeded(with: chatBot)

        let messages = builder().enumerated().map { offset, content in
            Chat.LocalMessage(
                messageId: UUID().uuidString,
                chatId: .chatExtension(chatBot.identifier),
                origin: .chatExtension(chatBot.identifier),
                creationSource: .localDevice,
                status: .incoming(.new),
                timestamp: Date().toChatTimestamp() + UInt64(offset),
                content: content,
                reactions: [],
                compactionId: nil,
                relatedMessages: []
            )
        }

        try await messageRepository.saveOperation({ messages }, { [] }).asyncExecute()

        settings.markWelcomeMessageSent(from: chatBot.identifier)
    }

    func getMessages(
        of type: Chat.LocalMessage.Content.ContentType
    ) async throws -> [Chat.LocalMessage] {
        async let allMessages = try messageRepository.fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .lazy
        return try await allMessages.filter { $0.content.contentType == type }
    }

    func getMessagesByContentKey(
        _ contentKey: String,
        with chatBot: ChatExtensionBotProtocol?
    ) async throws -> [Chat.LocalMessage] {
        let byContentKey = NSPredicate.messageByContentKey(contentKey)

        let filter: NSPredicate

        if let chatBot {
            let byChat = NSPredicate.localMessages(from: .chatExtension(chatBot.identifier))
            filter = NSCompoundPredicate(andPredicateWithSubpredicates: [byContentKey, byChat])
        } else {
            filter = byContentKey
        }

        let sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDChatMessage.timestamp),
                ascending: true
            )
        ]

        return try await storageFacade.createRepository(
            filter: filter,
            sortDescriptors: sortDescriptors,
            mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
        )
        .fetchAllOperation(with: RepositoryFetchOptions())
        .asyncExecute()
    }

    func modifyMessageContent(
        messageId: Chat.MessageId,
        content: Chat.LocalMessage.Content
    ) async throws {
        guard
            let message = try await messageRepository.fetchOperation(
                by: { messageId },
                options: RepositoryFetchOptions()
            )
            .asyncExecute()
        else {
            return
        }

        try await messageRepository.saveOperation({ [message.replacingContent(content)] }, { [] }).asyncExecute()
    }

    func hasResponse(
        from extension: any ChatExtending,
        with id: ChatExtension.ActionId
    ) async throws -> Bool {
        settings.hasActionResponse(from: `extension`, for: id)
    }

    func addActionResponse(
        _ response: String,
        action: ChatExtension.ActionId,
        delayDelivery: MessageDeliveryDelay,
        chatExtension: ChatExtending
    ) async throws {
        await delayDelivery.delay()

        settings.mark(action: action, for: chatExtension)

        let newMessage = Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: .chatExtension(chatExtension.identifier),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: Date().toChatTimestamp(),
            content: .extensionActionResponse(response, action),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        try await messageRepository.saveOperation({ [newMessage] }, { [] }).asyncExecute()
    }

    func hasDeliveredWelcomeMessages(for chatBot: ChatExtensionBotProtocol) async -> Bool {
        settings.hasWelcomeMessage(from: chatBot.identifier)
    }

    func markWelcomeMessagesDelivered(for chatBot: ChatExtensionBotProtocol) async {
        settings.markWelcomeMessageSent(from: chatBot.identifier)
    }

    // MARK: - Room Management

    func createRoom(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        name: String?,
        icon: String?
    ) async throws -> CreateRoomStatus {
        let chatId = Chat.Id.chatExtension(chatBot.identifier, roomId: roomId)

        let existingChat = try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()

        let roomMetadata = Chat.RoomMetadata(
            chatRelativeId: roomId,
            name: name,
            icon: icon
        )

        // A product registers its room on every start and truapi has no call that updates one, so
        // this is how a room placed before the product ran (`HostPlacedRoomPlacer`) gets its icon.
        if let existingChat {
            if existingChat.roomMetadata != roomMetadata {
                let update = Chat.RoomMetadataUpdate(chatId: chatId, metadata: roomMetadata)
                try await roomMetadataRepository.saveOperation({ [update] }, { [] }).asyncExecute()
            }

            return .exists
        }

        let chat = Chat.LocalModel.newChatWithRoom(
            extensionId: chatBot.identifier,
            roomId: roomId,
            roomMetadata: roomMetadata
        )

        try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()

        return .new
    }

    func subscribeRooms(
        for chatBot: ChatExtensionBotProtocol
    ) async -> AnyAsyncSequence<[RoomInfo]> {
        let predicate = NSPredicate.roomChatsForExtension(chatBot.identifier)
        return chatsProviderFactory
            .subscribeChatsWithPredicate(predicate)
            .map { chats -> [RoomInfo] in
                chats.compactMap { chat in
                    guard let metadata = chat.roomMetadata else { return nil }
                    return RoomInfo(
                        roomId: metadata.chatRelativeId,
                        name: metadata.name,
                        icon: metadata.icon,
                        participation: .roomHost
                    )
                }
            }
            .eraseToAnyAsyncSequence()
    }

    @discardableResult
    func sendNewMessage(
        from chatBot: ChatExtensionBotProtocol,
        roomId: String,
        newContent: Chat.LocalMessage.Content,
        messageDeliveryDelay: MessageDeliveryDelay
    ) async throws -> Chat.LocalMessage {
        await messageDeliveryDelay.delay()

        let message = Chat.LocalMessage.newExtensionMessage(
            chatBot.identifier,
            roomId: roomId,
            content: newContent
        )

        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        return message
    }
}
