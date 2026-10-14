import Foundation

protocol CompactedMessageExpansionServicing {
    func start()
    func stop()
}

final class CompactedMessageExpansionService {
    private let claimer: ChatMessageClaiming
    private let messageProviderFactory: ChatMessageDataProviderMaking
    private let context: CompactedMessageExpansionContext
    private let logger: LoggerProtocol

    private var monitoringTask: Task<Void, Never>?

    init(
        claimer: ChatMessageClaiming,
        messageProviderFactory: ChatMessageDataProviderMaking,
        context: CompactedMessageExpansionContext,
        logger: LoggerProtocol
    ) {
        self.claimer = claimer
        self.messageProviderFactory = messageProviderFactory
        self.context = context
        self.logger = logger
    }
}

// MARK: - CompactedMessageExpansionServicing

extension CompactedMessageExpansionService: CompactedMessageExpansionServicing {
    func start() {
        logger.debug("Starting expansion service")

        monitoringTask?.cancel()

        monitoringTask = Task { [weak self] in
            guard let self else { return }

            do {
                let stream = messageProviderFactory.subscribeMessages(
                    with: .incomingCompactedMessages()
                )

                for try await messages in stream {
                    logger.debug("Messages to expand: \(messages.count)")

                    for message in messages {
                        await performExpansionIfNeeded(for: message)
                    }
                }
            } catch {
                logger.error("Compacted message monitoring failed: \(error)")
            }
        }
    }

    func stop() {
        logger.debug("Stopping expansion service")

        monitoringTask?.cancel()
        monitoringTask = nil

        Task {
            await context.cancelAll()
        }
    }
}

// MARK: - Expansion

private extension CompactedMessageExpansionService {
    func performExpansionIfNeeded(for message: Chat.LocalMessage) async {
        let messageId = message.messageId

        await context.processExpansion(messageId: messageId) { [weak self] in
            Task { [weak self] in
                await self?.expand(message: message)
            }
        }
    }

    func expand(message: Chat.LocalMessage) async {
        let messageId = message.messageId

        logger.debug("Starting expansion for compacted message \(messageId)")

        do {
            try await claimer.claim(message: message) { [context, logger] expandedMessages in
                let result = CompactedExpansionMessageMapper.Model(
                    compactedMessage: message,
                    expandedMessages: expandedMessages
                )

                try await context.handleCompleted(messageId: messageId, result: result)

                logger.debug(
                    "Expanded compacted message \(messageId) into \(expandedMessages.count) messages"
                )
            }
        } catch {
            guard !Task.isCancelled else { return }

            logger.error("Failed to expand compacted message \(messageId): \(error)")

            await context.handleFailed(messageId: messageId)
        }
    }
}
