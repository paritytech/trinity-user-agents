import UIKit

final class SSOCreateTransactionHandler: SSORequestHandling {
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let signingHandler: TransactionSigningHandling
    private let pairedDeviceNames: PairedDeviceNameResolving
    private let logger: LoggerProtocol

    init(
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        signingHandler: TransactionSigningHandling,
        pairedDeviceNames: PairedDeviceNameResolving = PairedDeviceNameResolver(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.messageSender = messageSender
        self.signingHandler = signingHandler
        self.pairedDeviceNames = pairedDeviceNames
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .createTransactionRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .createTransactionRequest(value) = message.latestContent() else {
            return
        }

        logger.info("Will start create transaction")

        let pairedDeviceName = await pairedDeviceNames.deviceName(forStatementAccountId: host.accountId)

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let context = QueuedCreateTransactionContext(
                host: host,
                requester: PolkadotSigningRequester(
                    productId: value.caller.productId,
                    pairedDeviceName: pairedDeviceName
                ),
                requestMessageId: message.messageId,
                signingModel: .createTransaction(value.payload.toDomainPayload()),
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
                    self.logger.error("Signing handler failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }
}
