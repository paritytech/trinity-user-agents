import Foundation
import TrUAPIHost

struct SSOTrUAPIRequest: HostMessageIdentifiable {
    let message: SSORawHostMessage
    let service: any NativeSsoAccountHolderServiceProtocol

    var messageId: String { message.messageId }
}

final class SSOTrUAPIRequestHandler: SSORequestHandling {
    private let sender: any PolkadotHostMessageSending<SSORawHostMessage>
    private let disconnectApplier: SSORemoteDisconnectApplying
    private let logger: LoggerProtocol

    init(
        sender: any PolkadotHostMessageSending<SSORawHostMessage>,
        disconnectApplier: SSORemoteDisconnectApplying,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.sender = sender
        self.disconnectApplier = disconnectApplier
        self.logger = logger
    }

    func canHandle(_: SSOTrUAPIRequest) -> Bool {
        true
    }

    func handle(message request: SSOTrUAPIRequest, from host: PolkadotSignInHost) async {
        let messageId = request.messageId
        do {
            let outcome = try await request.service.handleSsoRequest(message: request.message.rawBytes)
            switch outcome {
            case let .response(responseBytes):
                let response = try SSORawHostMessage(rawBytes: responseBytes)
                try await sender.postMessage(response, to: host)
            case .disconnected:
                logger.info("Runtime disconnected for host \(host.name); running teardown")
                await disconnectApplier.applyDisconnect(from: host)
            case .ignored:
                logger.debug("Runtime ignored message \(messageId)")
            }
        } catch {
            logger.error("handleSsoRequest error for \(messageId): \(error)")
        }
    }
}
