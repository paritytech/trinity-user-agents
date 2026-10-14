import Foundation
import KeyDerivation

final class SSOCreateTransactionLegacyHandler: SSORequestHandling {
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
        guard case .createTransactionLegacyRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .createTransactionLegacyRequest(value) = message.latestContent() else {
            return
        }

        logger.info("Will start legacy create transaction")

        let payload = value.toDomainPayload()

        do {
            try accountResolver.resolveWallet(for: payload.signer.accountId)
        } catch {
            logger.error("Legacy create transaction account resolution failed: \(error)")
            await sendRejection(requestMessageId: message.messageId, to: host)
            return
        }

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let context = QueuedCreateTransactionContext(
                host: host,
                requestMessageId: message.messageId,
                signingModel: .legacyCreateTransaction(payload: payload),
                messageSender: messageSender,
                onCompleted: { continuation.resume() }
            )

            Task {
                do {
                    try await self.signingHandler.sponsorAndPresent(
                        model: context.signingModel,
                        context: context
                    )
                } catch {
                    self.logger.error("Legacy create transaction handler failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }
}

private extension SSOCreateTransactionLegacyHandler {
    func sendRejection(requestMessageId: String, to host: PolkadotSignInHost) async {
        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.createTransactionResponse(
                requestMessageId: requestMessageId,
                result: .failure(PolkadotSigningFailureReason.rejected)
            ))
        )

        do {
            try await messageSender.postMessage(message, to: host)
        } catch {
            logger.error("Failed to send legacy create transaction rejection: \(error)")
        }
    }
}
