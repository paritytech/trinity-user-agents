import Foundation
import Operation_iOS
import AsyncExtensions
import AsyncAlgorithms

/// The messages of a room that the host database keeps: person chats and host-store product rooms.
///
/// The whole chat is in the window, so both ends of the window are always at the end of the room. The source
/// gives each message to the extensions of the room once per session, as the room screen did before the seam.
final class CoreDataMessageSource: ChatMessageSourcing, @unchecked Sendable {
    private let chatId: Chat.Id
    private let chatExtensionRegistry: ChatExtensionsRegistering
    private let messageProviderFactory: ChatMessageDataProviderMaking
    private let historyRepository: AnyDataProviderRepository<ChatExtension.ProcessingHistory>
    private let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    private let messageStatusUpdateRepository: AnyDataProviderRepository<Chat.ChatMessageStatusUpdate>
    private let logger: LoggerProtocol

    init(
        chatId: Chat.Id,
        chatExtensionRegistry: ChatExtensionsRegistering,
        messageProviderFactory: ChatMessageDataProviderMaking,
        historyRepository: AnyDataProviderRepository<ChatExtension.ProcessingHistory>,
        messageRepository: AnyDataProviderRepository<Chat.LocalMessage>,
        messageStatusUpdateRepository: AnyDataProviderRepository<Chat.ChatMessageStatusUpdate>,
        logger: LoggerProtocol
    ) {
        self.chatId = chatId
        self.chatExtensionRegistry = chatExtensionRegistry
        self.messageProviderFactory = messageProviderFactory
        self.historyRepository = historyRepository
        self.messageRepository = messageRepository
        self.messageStatusUpdateRepository = messageStatusUpdateRepository
        self.logger = logger
    }

    /// The messages of the chat, each time the database changes.
    func messages() async throws -> AnyAsyncSequence<[Chat.LocalMessage]> {
        let chatProcessingHistory = try await historyRepository.fetchAllOperation(
            with: RepositoryFetchOptions()
        )
        .asyncExecute()

        let processingContext = ChatExtensionProcessingContext(
            chatId: chatId,
            initialHistory: chatProcessingHistory,
            messageRepository: messageRepository,
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

    func snapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        try await messages()
            .map { MessageListSnapshot(messages: $0) }
            .eraseToAnyAsyncSequence()
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

    func reload() async {}

    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome {
        switch action {
        case let .send(content):
            try await save(content: content)
        case let .markSeen(messageIds):
            try await markAsSeen(messageIds: messageIds)
        case .toggleReaction,
             .contactAction:
            throw ChatMessageSourceError.unsupported
        }

        return .done
    }
}

private extension CoreDataMessageSource {
    func save(content: Chat.LocalMessage.Content) async throws {
        let newMessage = Chat.LocalMessage
            .newMessage(to: chatId, content: content)
            .replacingStatus(initialStatus)

        try await messageRepository.saveOperation({ [newMessage] }, { [] }).asyncExecute()
    }

    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws {
        let updates = messageIds.map { Chat.ChatMessageStatusUpdate(messageId: $0, status: .incoming(.seen)) }

        guard !updates.isEmpty else {
            return
        }

        try await messageStatusUpdateRepository.saveOperation({ updates }, { [] }).asyncExecute()
    }

    var initialStatus: Chat.LocalMessage.Status {
        switch chatId {
        case .chatExtension:
            .outgoing(.delivered)
        case .person:
            .outgoing(.new)
        }
    }
}
