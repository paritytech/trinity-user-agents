import Foundation
import Products

final class SSOCreateProofHandler: SSORequestHandling {
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
        guard case .createProofRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .createProofRequest(request) = message.latestContent() else {
            return
        }

        logger.info("Create proof request received from \(host.name)")

        let proofResult: PolkadotHostRemoteMessage.CreateProofResult

        do {
            let handler = handlerFactory.makeCreateProofHandler(callingProductId: request.callingProductId)
            let proof = try await handler.createProof(
                context: request.context,
                ring: request.ring,
                message: request.message
            )
            proofResult = .success(proof)
        } catch {
            logger.error("Failed to create proof: \(error)")
            proofResult = .failure(.wrapping(error))
        }

        let responseMessage = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.createProofResponse(
                requestMessageId: message.messageId,
                result: proofResult
            ))
        )

        do {
            try await messageSender.postMessage(responseMessage, to: host)
        } catch {
            logger.error("Failed to send create proof response: \(error)")
        }
    }
}
