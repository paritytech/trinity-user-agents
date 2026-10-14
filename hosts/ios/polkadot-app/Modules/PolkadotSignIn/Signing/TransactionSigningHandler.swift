import Foundation
import Products
import ChainRegistry

enum TransactionSigningHandlerError: Error {
    case signingUnavailable
}

protocol TransactionSigningHandling {
    func sponsorAndPresent(
        model: PolkadotHostSigningModel,
        context: PolkadotSigningContextProtocol
    ) async throws
}

final class TransactionSigningHandler: TransactionSigningHandling {
    private let pgasSponsor: PGasTransactionSponsoring
    private let chainRegistry: ChainRegistryProtocol
    private let router: ProductsRouting
    private let logger: LoggerProtocol

    init(
        pgasSponsor: PGasTransactionSponsoring,
        chainRegistry: ChainRegistryProtocol,
        router: ProductsRouting,
        logger: LoggerProtocol
    ) {
        self.pgasSponsor = pgasSponsor
        self.chainRegistry = chainRegistry
        self.router = router
        self.logger = logger
    }

    /// Sponsors pGas if needed, then presents the signing UI.
    /// Throws `TransactionSigningHandlerError.signingUnavailable` if UI presentation fails.
    func sponsorAndPresent(
        model: PolkadotHostSigningModel,
        context: PolkadotSigningContextProtocol
    ) async throws {
        try await tryPGasSponsoring(for: model)

        let presented = await MainActor.run {
            router.showSigning(with: context)
        }

        guard presented else {
            throw TransactionSigningHandlerError.signingUnavailable
        }
    }
}

// MARK: - PGas Sponsoring

private extension TransactionSigningHandler {
    func tryPGasSponsoring(for model: PolkadotHostSigningModel) async throws {
        switch model {
        case let .signingRequest(request):
            try await tryPGasSponsoring(request)
        case let .createTransaction(payload):
            try await sponsorTransaction(
                account: payload.signer,
                callData: payload.callData,
                genesisHash: payload.genesisHash
            )
        case .legacyRawPayload,
             .legacyCreateTransaction,
             .legacySignPayload:
            break
        }
    }

    func tryPGasSponsoring(_ request: PolkadotHostRemoteMessage.SigningRequest) async throws {
        switch request {
        case let .transaction(payload):
            try await sponsorTransaction(
                account: payload.account,
                callData: payload.method,
                genesisHash: payload.genesisHash
            )
        case .rawPayload:
            break
        }
    }

    func sponsorTransaction(
        account: ProductAccountId,
        callData: Data,
        genesisHash: Data
    ) async throws {
        let genesisHex = genesisHash.toHex()
        let chainId = chainRegistry.getChainByGenesis(for: genesisHex)?.chainId ?? genesisHex

        logger.debug("Will try sponsor pGas for chain \(chainId)")

        try await pgasSponsor.sponsorIfNeeded(
            productAccount: account,
            callData: callData,
            chainId: chainId
        )

        logger.debug("PGas sponsorship done")
    }
}
