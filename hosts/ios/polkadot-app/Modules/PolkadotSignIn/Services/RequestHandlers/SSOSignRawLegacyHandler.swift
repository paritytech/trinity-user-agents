import Foundation
import KeyDerivation

final class SSOSignRawLegacyHandler: SSORequestHandling {
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let signingHandler: TransactionSigningHandling
    private let accountResolver: IdentityAccountResolving
    private let logger: LoggerProtocol

    init(
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        signingHandler: TransactionSigningHandling,
        accountResolver: IdentityAccountResolving = IdentityAccountResolver(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.messageSender = messageSender
        self.signingHandler = signingHandler
        self.accountResolver = accountResolver
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .signRawLegacyRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .signRawLegacyRequest(value) = message.latestContent() else {
            return
        }

        do {
            try accountResolver.resolveWallet(for: value.account)
        } catch {
            logger.error("Legacy sign raw account resolution failed: \(error)")
            await sendRejection(requestMessageId: message.messageId, to: host)
            return
        }

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let context = QueuedSsoSignRawLegacyContext(
                host: host,
                requestMessageId: message.messageId,
                signingModel: .legacyRawPayload(account: value.account, type: value.type),
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
                    self.logger.error("Legacy sign raw handler failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }
}

private extension SSOSignRawLegacyHandler {
    func sendRejection(requestMessageId: String, to host: PolkadotSignInHost) async {
        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signRawLegacyResponse(
                requestMessageId: requestMessageId,
                result: .failure(PolkadotSigningFailureReason.rejected)
            ))
        )

        do {
            try await messageSender.postMessage(message, to: host)
        } catch {
            logger.error("Failed to send legacy sign raw rejection: \(error)")
        }
    }
}
