import Foundation
import Products

final class SSOAliasRequestHandler: SSORequestHandling {
    private let handlerFactory: APPersonhoodHandlerMaking
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let logger: LoggerProtocol

    init(
        handlerFactory: APPersonhoodHandlerMaking,
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.handlerFactory = handlerFactory
        self.messageSender = messageSender
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .aliasRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .aliasRequest(request) = message.latestContent() else {
            return
        }

        logger.info("Alias request received from \(host.name)")

        let aliasResult: PolkadotHostRemoteMessage.AliasResult

        do {
            let handler = handlerFactory.makeAliasHandler(callingProductId: request.callingProductId)
            let alias = try await handler.getContextualAlias(
                context: request.context,
                ring: request.ring
            )
            aliasResult = .success(alias)
        } catch {
            logger.error("Failed to get contextual alias: \(error)")
            aliasResult = .failure(.wrapping(error))
        }

        let responseMessage = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.aliasResponse(
                requestMessageId: message.messageId,
                result: aliasResult
            ))
        )

        do {
            try await messageSender.postMessage(responseMessage, to: host)
        } catch {
            logger.error("Failed to send alias response: \(error)")
        }
    }
}
