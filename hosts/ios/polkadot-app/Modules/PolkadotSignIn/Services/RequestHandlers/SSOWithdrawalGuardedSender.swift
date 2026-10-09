import Foundation
import MessageExchangeKit

/// Posts through `sender`, except a response to a request the pairing host has
/// withdrawn, which is dropped: a withdrawn request posts no response.
final class SSOWithdrawalGuardedSender: PolkadotHostMessageSending {
    typealias Message = PolkadotHostRemoteMessage

    private let sender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let withdrawnRequests: SSOWithdrawnRequests
    private let logger: LoggerProtocol

    init(
        sender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        withdrawnRequests: SSOWithdrawnRequests,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.sender = sender
        self.withdrawnRequests = withdrawnRequests
        self.logger = logger
    }

    func setExchangeService(
        _ service: AnyMessageExchangeService<OpaqueMessageWrapper<PolkadotHostRemoteMessage>>
    ) async {
        await sender.setExchangeService(service)
    }

    func postMessage(_ message: PolkadotHostRemoteMessage, to host: PolkadotSignInHost) async throws {
        if let requestId = message.respondingTo, withdrawnRequests.contains(requestId) {
            logger.info("Not responding to withdrawn request \(requestId)")
            return
        }

        try await sender.postMessage(message, to: host)
    }

    func handleDidPostMessages(_ messages: [PolkadotHostRemoteMessage], withError error: Error?) async {
        await sender.handleDidPostMessages(messages, withError: error)
    }

    func cancelPendingMessages(excluding retainedMessageIds: Set<String>) async {
        await sender.cancelPendingMessages(excluding: retainedMessageIds)
    }
}
