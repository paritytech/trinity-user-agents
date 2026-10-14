import Foundation
import KeyDerivation
import Products

protocol APSignVrfHandling: Sendable {
    func signVrf(_ payload: SignVrfPayload) async throws -> VrfSignature
}

typealias SignVrfWalletFactory = @Sendable (ProductAccountId) throws -> RawKeypairProviding

final class APSignVrfHandler: @unchecked Sendable {
    private let callingProductId: ProductId?
    private let confirmationRequester: SignVrfConfirming
    private let walletFactory: SignVrfWalletFactory
    private let logger: LoggerProtocol

    init(
        callingProductId: ProductId?,
        confirmationRequester: SignVrfConfirming,
        walletFactory: @escaping SignVrfWalletFactory,
        logger: LoggerProtocol
    ) {
        self.callingProductId = callingProductId
        self.confirmationRequester = confirmationRequester
        self.walletFactory = walletFactory
        self.logger = logger
    }
}

extension APSignVrfHandler: APSignVrfHandling {
    func signVrf(_ payload: SignVrfPayload) async throws -> VrfSignature {
        do {
            return try await performSignVrf(payload)
        } catch {
            logger.error("Sign VRF failed: \(error)")
            throw SignVrfError.wrapping(error)
        }
    }
}

private extension APSignVrfHandler {
    func performSignVrf(_ payload: SignVrfPayload) async throws -> VrfSignature {
        try payload.validateTranscriptSize()

        try await confirmSigning(of: payload)

        return try Sr25519VrfSigner.sign(
            wallet: walletFactory(payload.account),
            transcriptLabel: payload.transcriptLabel,
            items: payload.items
        )
    }

    func confirmSigning(of payload: SignVrfPayload) async throws {
        let request = SignVrfConfirmationRequest(
            callingProductId: callingProductId,
            payload: payload
        )

        guard await confirmationRequester.confirm(request) == .approved else {
            throw SignVrfError.rejected
        }
    }
}
