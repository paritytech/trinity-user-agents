import Foundation
import KeyDerivation
import Products
import SubstrateSdk

final class QueuedSsoSignRawLegacyContext: PolkadotSigningContextProtocol {
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

        guard case let .rawSignature(rawSignature) = result else {
            logger.warning("Unexpected result for legacy sign raw flow")
            return
        }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signRawLegacyResponse(
                requestMessageId: requestMessageId,
                result: .success(rawSignature)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }

    func rejectRequest() async throws {
        defer { complete() }

        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.signRawLegacyResponse(
                requestMessageId: requestMessageId,
                result: .failure(PolkadotSigningFailureReason.rejected)
            ))
        )

        try await messageSender.postMessage(message, to: host)
    }
}

private extension QueuedSsoSignRawLegacyContext {
    func complete() {
        guard !didComplete else { return }
        didComplete = true
        onCompleted()
    }
}
