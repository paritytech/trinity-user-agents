import Foundation
import Products

/// RFC-0023 `sign_vrf` over the SSO session. Bounds and the confirmation prompt live in the
/// shared ``APSignVrfHandling`` — nothing here is SSO-specific beyond the wire mapping.
final class SSOSignVrfRequestHandler: SSORequestHandling {
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
        guard case .signVrfRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .signVrfRequest(request) = message.latestContent() else {
            return
        }

        logger.info("Sign VRF request received from \(host.name)")

        let result: PolkadotHostRemoteMessage.SignVrfHostResult

        do {
            let handler = handlerFactory.makeSignVrfHandler(callingProductId: request.callingProductId)
            let signature = try await handler.signVrf(request.payload)
            result = .success(.init(signature: signature))
        } catch {
            logger.error("Failed to sign VRF: \(error)")
            result = .failure(.init(wrapping: error))
        }

        let responseMessage = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signVrfResponse(
                requestMessageId: message.messageId,
                result: result
            ))
        )

        do {
            try await messageSender.postMessage(responseMessage, to: host)
        } catch {
            logger.error("Failed to send sign VRF response: \(error)")
        }
    }
}
