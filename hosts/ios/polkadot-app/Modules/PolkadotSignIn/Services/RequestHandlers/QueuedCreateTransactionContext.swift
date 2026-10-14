import Foundation
import Products
import SubstrateSdk
import KeyDerivation

final class QueuedCreateTransactionContext: PolkadotSigningContextProtocol {
    private let host: PolkadotSignInHost
    private let requestMessageId: String
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let onCompleted: () -> Void

    let requester: PolkadotSigningRequester
    let signingModel: PolkadotHostSigningModel

    private var didComplete = false

    init(
        host: PolkadotSignInHost,
        requestMessageId: String,
        signingModel: PolkadotHostSigningModel,
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        onCompleted: @escaping () -> Void
    ) {
        self.host = host
        self.requestMessageId = requestMessageId
        self.signingModel = signingModel
        self.messageSender = messageSender
        self.onCompleted = onCompleted
        requester = PolkadotSigningRequester(name: host.name, iconUrl: host.iconUrl)
    }

    deinit {
        complete()
    }

    func sendResult(_ result: PolkadotHostSigningResult) async throws {
        defer { complete() }

        guard case let .signedTransaction(encodedTransaction) = result else {
            fatalError("createTransaction handler expects signedTransaction result")
        }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.createTransactionResponse(
                requestMessageId: requestMessageId,
                result: .success(encodedTransaction)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }

    func rejectRequest() async throws {
        defer { complete() }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.createTransactionResponse(
                requestMessageId: requestMessageId,
                result: .failure(PolkadotSigningFailureReason.rejected)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }
}

private extension QueuedCreateTransactionContext {
    func complete() {
        guard !didComplete else { return }
        didComplete = true
        onCompleted()
    }
}
