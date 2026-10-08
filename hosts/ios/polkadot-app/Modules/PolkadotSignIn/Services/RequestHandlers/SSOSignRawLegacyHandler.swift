import Foundation
import KeyDerivation

final class SSOSignRawLegacyHandler: SSORequestHandling {
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let signingHandler: TransactionSigningHandling
    private let pairedDeviceNames: PairedDeviceNameResolving
    private let accountResolver: IdentityAccountResolving
    private let logger: LoggerProtocol

    init(
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        signingHandler: TransactionSigningHandling,
        pairedDeviceNames: PairedDeviceNameResolving = PairedDeviceNameResolver(),
        accountResolver: IdentityAccountResolving = IdentityAccountResolver(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.messageSender = messageSender
        self.signingHandler = signingHandler
        self.pairedDeviceNames = pairedDeviceNames
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
            try accountResolver.resolveWallet(for: value.payload.account)
        } catch {
            logger.error("Legacy sign raw account resolution failed: \(error)")
            await sendRejection(requestMessageId: message.messageId, to: host)
            return
        }

        let pairedDeviceName = await pairedDeviceNames.deviceName(forStatementAccountId: host.accountId)

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let context = QueuedSsoSignRawLegacyContext(
                host: host,
                requester: PolkadotSigningRequester(
                    productId: value.caller.productId,
                    pairedDeviceName: pairedDeviceName
                ),
                requestMessageId: message.messageId,
                signingModel: .legacyRawPayload(account: value.payload.account, type: value.payload.type),
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
