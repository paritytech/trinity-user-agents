import Foundation
import MessageExchangeKit
import Operation_iOS

protocol PolkadotHostMessageHandling<Message> {
    associatedtype Message
    func handleMessages(
        _ messages: [Message],
        from host: PolkadotSignInHost
    ) async
}

final class PolkadotHostMessageHandler {
    private let processingContext: SSORequestProcessingContext<PolkadotHostRemoteMessage>
    private let handledRequestRepository: AnyDataProviderRepository<SSOHandledRequest>
    private let logger: LoggerProtocol

    init(
        processingContext: SSORequestProcessingContext<PolkadotHostRemoteMessage>,
        handledRequestRepositoryFactory: SSOHandledRequestRepositoryMaking = SSOHandledRequestRepositoryFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.processingContext = processingContext
        handledRequestRepository = handledRequestRepositoryFactory.createRepository()
        self.logger = logger
    }
}

extension PolkadotHostMessageHandler: PolkadotHostMessageHandling {
    func handleMessages(
        _ messages: [PolkadotHostRemoteMessage],
        from host: PolkadotSignInHost
    ) async {
        let newMessages = await filterAlreadyHandled(messages)

        logger.info("New messages: \(newMessages.count)")

        for message in newMessages {
            guard let content = message.latestContent() else {
                logger.error("Failed to get content for message \(message.messageId)")
                continue
            }

            if isResponseMessage(content) {
                continue
            }

            await processingContext.enqueue(message: message, from: host)
        }

        await markMessagesAsHandled(newMessages)

        logger.info("Did mark messages as handled: \(newMessages)")
    }
}

private extension PolkadotHostMessageHandler {
    func isResponseMessage(_ content: PolkadotHostRemoteMessage.LatestContent) -> Bool {
        switch content {
        case .signingResponse,
             .aliasResponse,
             .resourceAllocationResponse,
             .createTransactionResponse,
             .productSubtreeResponse:
            true
        default:
            false
        }
    }

    func filterAlreadyHandled(
        _ messages: [PolkadotHostRemoteMessage]
    ) async -> [PolkadotHostRemoteMessage] {
        var result = [PolkadotHostRemoteMessage]()

        for message in messages {
            do {
                let existing = try await handledRequestRepository
                    .fetchOperation(
                        by: { message.messageId },
                        options: RepositoryFetchOptions()
                    )
                    .asyncExecute()

                if existing == nil {
                    result.append(message)
                } else {
                    logger.debug("Skipping already handled message \(message.messageId)")
                }
            } catch {
                logger.error("Failed to check handled request \(message.messageId): \(error)")
                result.append(message)
            }
        }

        return result
    }

    func markMessagesAsHandled(_ messages: [PolkadotHostRemoteMessage]) async {
        let records = messages.map { SSOHandledRequest(messageId: $0.messageId) }

        guard !records.isEmpty else {
            return
        }

        do {
            try await handledRequestRepository
                .saveOperation({ records }, { [] })
                .asyncExecute()
        } catch {
            logger.error("Failed to save handled requests: \(error)")
        }
    }
}
