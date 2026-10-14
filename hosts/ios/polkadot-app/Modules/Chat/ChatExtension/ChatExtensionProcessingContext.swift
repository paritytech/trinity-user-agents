import Foundation
import Operation_iOS
import AsyncExtensions

actor ChatExtensionProcessingContext {
    typealias MessageOverrides = [Chat.MessageId: Chat.LocalMessage.Content]

    struct HistoryItem: Hashable {
        let messageId: Chat.MessageId
        let extensionId: ChatExtension.Id
    }

    private var processedHistory: Set<HistoryItem>
    private var pendingHistory: Set<HistoryItem> = []
    private var processedMessages = Set<Chat.MessageId>()
    private var messageOverrides: AsyncCurrentValueSubject<MessageOverrides> = .init([:])

    let chatId: Chat.Id
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let attachmentsUpdateRepository: AnyDataProviderRepository<Chat.AttachmentUploadingUpdate>
    let processingHistoryRepository: AnyDataProviderRepository<ChatExtension.ProcessingHistory>
    let logger: LoggerProtocol

    init(
        chatId: Chat.Id,
        initialHistory: [ChatExtension.ProcessingHistory],
        messageRepository: AnyDataProviderRepository<Chat.LocalMessage>,
        attachmentsUpdateRepository: AnyDataProviderRepository<Chat.AttachmentUploadingUpdate>,
        processingHistoryRepository: AnyDataProviderRepository<ChatExtension.ProcessingHistory>,
        logger: LoggerProtocol
    ) {
        self.chatId = chatId
        processedHistory = Set(initialHistory.map { HistoryItem(messageId: $0.messageId, extensionId: $0.extensionId) })
        self.messageRepository = messageRepository
        self.attachmentsUpdateRepository = attachmentsUpdateRepository
        self.processingHistoryRepository = processingHistoryRepository
        self.logger = logger
    }
}

extension ChatExtensionProcessingContext: ChatExtensionProcessingContextProtocol {
    func addNewMessage(
        _ content: Chat.LocalMessage.Content,
        delayDelivery: MessageDeliveryDelay,
        chatExtension: ChatExtending
    ) async throws {
        await delayDelivery.delay()

        let newMessage = Chat.LocalMessage.newExtensionMessage(chatExtension.identifier, content: content)
        try await messageRepository.saveOperation({ [newMessage] }, { [] }).asyncExecute()
    }

    func modifyMessageContent(
        messageId: Chat.MessageId,
        content: Chat.LocalMessage.Content
    ) async throws {
        var newOverrides = messageOverrides.value
        newOverrides[messageId] = content
        messageOverrides.send(newOverrides)
    }
}

extension ChatExtensionProcessingContext {
    var messageOverridesSequence: AnyAsyncSequence<MessageOverrides> {
        messageOverrides.eraseToAnyAsyncSequence()
    }

    func hasProcessed(messageId: Chat.MessageId, extensionId: ChatExtension.Id) -> Bool {
        processedHistory.contains(.init(messageId: messageId, extensionId: extensionId))
    }

    func noteProcessingResult(
        messageId: Chat.MessageId,
        extensionId: ChatExtension.Id,
        result: ChatExtension.ProcessingResult
    ) {
        guard case .processed = result else {
            return
        }

        pendingHistory.insert(.init(messageId: messageId, extensionId: extensionId))
    }

    func flushChatExtensionHistory() async {
        guard !pendingHistory.isEmpty else {
            return
        }

        pendingHistory.forEach { processedHistory.insert($0) }

        let processedHistoryItems = pendingHistory.map { item in
            ChatExtension.ProcessingHistory(
                messageId: item.messageId,
                chatId: chatId.rawRepresentation,
                extensionId: item.extensionId
            )
        }

        do {
            try await processingHistoryRepository
                .saveOperation({ processedHistoryItems }, { [] })
                .asyncExecute()

            pendingHistory = []

            logger.debug("Flushed processing history")
        } catch {
            logger.error("Processing history flush failed: \(error)")
        }
    }

    func processOncePerSession(
        _ message: Chat.LocalMessage,
        processingClosure: (Chat.LocalMessage) async -> Void
    ) async {
        guard !processedMessages.contains(message.messageId) else {
            return
        }

        await processingClosure(message)

        processedMessages.insert(message.messageId)
    }
}

extension ChatExtensionProcessingContext.MessageOverrides {
    func applyOverrides(to message: Chat.LocalMessage) -> Chat.LocalMessage {
        guard let newContent = self[message.messageId] else {
            return message
        }

        return message.replacingContent(newContent)
    }
}
