import Foundation
import Products
import SubstrateSdk
import KeyDerivation

final class QueuedSsoSigningContext: PolkadotSigningContextProtocol {
    private let host: PolkadotSignInHost
    private let requestMessageId: String
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let onCompleted: () -> Void

    let requester: PolkadotSigningRequester
    let signingModel: PolkadotHostSigningModel
    let logger: LoggerProtocol

    private var didComplete = false

    init(
        host: PolkadotSignInHost,
        requestMessageId: String,
        signingModel: PolkadotHostSigningModel,
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        logger: LoggerProtocol,
        onCompleted: @escaping () -> Void
    ) {
        self.host = host
        self.requestMessageId = requestMessageId
        self.signingModel = signingModel
        self.messageSender = messageSender
        self.onCompleted = onCompleted
        self.logger = logger
        requester = PolkadotSigningRequester(name: host.name, iconUrl: host.iconUrl)
    }

    deinit {
        complete()
    }

    func sendResult(_ result: PolkadotHostSigningResult) async throws {
        defer { complete() }

        let signature: PolkadotHostRemoteMessage.Signature

        switch result {
        case let .signedPayload(rawSignature, signedTransaction):
            signature = PolkadotHostRemoteMessage.Signature(
                rawSignature: rawSignature,
                signedTransaction: signedTransaction
            )
        case let .rawSignature(rawSignature):
            signature = PolkadotHostRemoteMessage.Signature(
                rawSignature: rawSignature,
                signedTransaction: nil
            )
        case .signedTransaction:
            logger.warning("Received signedTransaction result for SSO signing flow")
            return
        }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signingResponse(
                requestMessageId: requestMessageId,
                result: .success(signature)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }

    func rejectRequest() async throws {
        defer { complete() }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signingResponse(
                requestMessageId: requestMessageId,
                result: .failure(PolkadotSigningFailureReason.rejected)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }
}

private extension QueuedSsoSigningContext {
    func complete() {
        guard !didComplete else { return }
        didComplete = true
        onCompleted()
    }
}
