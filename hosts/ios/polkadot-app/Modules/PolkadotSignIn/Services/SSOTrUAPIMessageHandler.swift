import Foundation
import Operation_iOS
import TrUAPIHost

final class SSOTrUAPIMessageHandler {
    private let processingContext: SSORequestProcessingContext<SSOTrUAPIRequest>
    private let handledRequestRepository: AnyDataProviderRepository<SSOHandledRequest>
    private let logger: LoggerProtocol

    init(
        processingContext: SSORequestProcessingContext<SSOTrUAPIRequest>,
        handledRequestRepositoryFactory: SSOHandledRequestRepositoryMaking = SSOHandledRequestRepositoryFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.processingContext = processingContext
        handledRequestRepository = handledRequestRepositoryFactory.createRepository()
        self.logger = logger
    }
}

extension SSOTrUAPIMessageHandler {
    func handleMessages(
        _ messages: [SSORawHostMessage],
        from host: PolkadotSignInHost,
        service: any NativeSsoAccountHolderServiceProtocol
    ) async {
        let newMessages = await filterAlreadyHandled(messages)

        logger.info("New raw messages: \(newMessages.count)")

        for message in newMessages {
            do {
                guard try service.handleSsoControl(message: message.rawBytes) == nil else { continue }
                await processingContext.enqueue(
                    message: SSOTrUAPIRequest(message: message, service: service),
                    from: host
                )
            } catch {
                logger.error("Invalid SSO control for \(message.messageId): \(error)")
            }
        }

        await markMessagesAsHandled(newMessages)

        logger.info("Did mark raw messages as handled: \(newMessages.count)")
    }
}

private extension SSOTrUAPIMessageHandler {
    func filterAlreadyHandled(
        _ messages: [SSORawHostMessage]
    ) async -> [SSORawHostMessage] {
        var result = [SSORawHostMessage]()

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

    func markMessagesAsHandled(_ messages: [SSORawHostMessage]) async {
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
