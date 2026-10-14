import Foundation
import TrUAPIHost

// MARK: - Runtime Abstraction

protocol SSOTruAPIRuntimeHandling: AnyObject, Sendable {
    func handleSsoRequest(message: Data) async throws -> SsoRequestOutcome
    func prepareDisconnectRequest() -> Data
}

extension TrUAPIHostRuntime: SSOTruAPIRuntimeHandling {}

// MARK: - Request Handler

final class SSOTrUAPIRequestHandler: SSORequestHandling {
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let sender: any PolkadotHostMessageSending<SSORawHostMessage>
    private let disconnectApplier: SSORemoteDisconnectApplying
    private let logger: LoggerProtocol

    init(
        runtimeProvider: TrUAPIHostRuntimeProviding,
        sender: any PolkadotHostMessageSending<SSORawHostMessage>,
        disconnectApplier: SSORemoteDisconnectApplying,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.runtimeProvider = runtimeProvider
        self.sender = sender
        self.disconnectApplier = disconnectApplier
        self.logger = logger
    }

    func canHandle(_: SSORawHostMessage) -> Bool {
        true
    }

    func handle(message: SSORawHostMessage, from host: PolkadotSignInHost) async {
        let messageId = message.messageId

        let runtime: any SSOTruAPIRuntimeHandling
        do {
            runtime = try runtimeProvider.sharedRuntime()
        } catch {
            logger.error("sharedRuntime unavailable (pre-onboarding?); dropping \(messageId): \(error)")
            return
        }

        do {
            let outcome = try await runtime.handleSsoRequest(message: message.rawBytes)
            await apply(outcome: outcome, messageId: messageId, to: host)
        } catch {
            logger.error("handleSsoRequest error for \(messageId): \(error)")
        }
    }
}

private extension SSOTrUAPIRequestHandler {
    func apply(outcome: SsoRequestOutcome, messageId: String, to host: PolkadotSignInHost) async {
        switch outcome {
        case let .response(responseBytes):
            await postResponse(responseBytes, messageId: messageId, to: host)
        case .disconnected:
            logger.info("Runtime disconnected for host \(host.name); running teardown")
            await disconnectApplier.applyDisconnect(from: host)
        case .ignored:
            logger.debug("Runtime ignored message \(messageId)")
        }
    }

    func postResponse(_ bytes: Data, messageId: String, to host: PolkadotSignInHost) async {
        logger.debug("Runtime response for \(messageId); posting back to \(host.name)")
        do {
            try await sender.postMessage(SSORawHostMessage(rawBytes: bytes), to: host)
        } catch {
            logger.error("Failed to post response for \(messageId): \(error)")
        }
    }
}
