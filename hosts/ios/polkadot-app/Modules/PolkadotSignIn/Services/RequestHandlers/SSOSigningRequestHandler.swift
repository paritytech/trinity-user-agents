import UIKit
import UIKitExt

final class SSOSigningRequestHandler: SSORequestHandling {
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let signingHandler: TransactionSigningHandling
    private let logger: LoggerProtocol

    init(
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        signingHandler: TransactionSigningHandling,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.messageSender = messageSender
        self.signingHandler = signingHandler
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .signingRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .signingRequest(value) = message.latestContent() else {
            return
        }

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let context = QueuedSsoSigningContext(
                host: host,
                requestMessageId: message.messageId,
                signingModel: .signingRequest(value),
                messageSender: messageSender,
                logger: logger,
                onCompleted: { continuation.resume() }
            )

            Task {
                do {
                    try await self.signingHandler.sponsorAndPresent(
                        model: context.signingModel,
                        context: context
                    )
                } catch {
                    self.logger.error("Signing handler failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }
}
