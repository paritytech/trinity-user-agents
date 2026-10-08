import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk
import SubstrateSdkExt

protocol ProductsSignConfirmModelMaking {
    /// Build the confirmation view model from a signing review. No wallet
    /// resolution — the rust core signs after approval. Best-effort: a call
    /// that cannot be decoded is shown as raw hex (the user reviews it), so
    /// throwing is reserved for genuinely corrupt raw payloads.
    func makeModel(
        from input: ProductsSignConfirmInput,
        requester: PolkadotSigningRequester
    ) async throws -> ProductsSignConfirmModel
}

struct ProductsSignConfirmModelFactory: ProductsSignConfirmModelMaking {
    private let chainRegistry: ChainRegistryProtocol
    private let renderer: PolkadotSigningCallRenderer

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        renderer: PolkadotSigningCallRenderer = PolkadotSigningCallRenderer()
    ) {
        self.chainRegistry = chainRegistry
        self.renderer = renderer
    }

    func makeModel(
        from input: ProductsSignConfirmInput,
        requester: PolkadotSigningRequester
    ) async throws -> ProductsSignConfirmModel {
        switch input {
        case let .signPayload(review):
            let (callBytes, genesisHash) = payloadCall(from: review)
            return try await makeTransactionModel(
                callBytes: callBytes,
                genesisHash: genesisHash,
                requester: requester
            )
        case let .createTransaction(review):
            let (callBytes, genesisHash) = transactionCall(from: review)
            return try await makeTransactionModel(
                callBytes: callBytes,
                genesisHash: genesisHash,
                requester: requester
            )
        case let .signRaw(review):
            let (payload, watermarked) = rawPayload(from: review)
            return try makeRawModel(payload: payload, watermarked: watermarked, requester: requester)
        }
    }
}

// MARK: - Model builders

private extension ProductsSignConfirmModelFactory {
    func makeTransactionModel(
        callBytes: Data,
        genesisHash: Data,
        requester: PolkadotSigningRequester
    ) async throws -> ProductsSignConfirmModel {
        let codingFactory = await codingFactory(forGenesis: genesisHash)
        let call = renderer.parseCall(from: callBytes, codingFactory: codingFactory)

        return try ProductsSignConfirmModel(
            requester: requester,
            descriptionText: call.descriptionText,
            detailsText: renderer.callDetailsText(call, codingFactory: codingFactory),
            isTransaction: true
        )
    }

    func makeRawModel(
        payload: RawPayload,
        watermarked: Bool,
        requester: PolkadotSigningRequester
    ) throws -> ProductsSignConfirmModel {
        let rawBytes: Data =
            switch payload {
            case let .bytes(data): data
            case let .payload(string): string.isHex() ? try Data(hexString: string) : Data(string.utf8)
            }
        let signingBytes = watermarked ? try renderer.wrappedBytes(rawBytes) : rawBytes

        return ProductsSignConfirmModel(
            requester: requester,
            descriptionText: watermarked ? "Raw bytes" : "Unprotected signature: may authorize transactions",
            detailsText: signingBytes.toHex(includePrefix: true),
            isTransaction: !watermarked
        )
    }

    /// Best-effort coding factory for the chain; `nil` when the chain is not
    /// synced or the runtime is unavailable (the call then renders as raw hex).
    func codingFactory(forGenesis genesisHash: Data) async -> RuntimeCoderFactoryProtocol? {
        guard
            let chain = chainRegistry.getChainByGenesis(for: genesisHash.toHex()),
            let runtimeProvider = chainRegistry.getRuntimeProvider(for: chain.chainId),
            let codingFactory = try? await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        else {
            return nil
        }

        return codingFactory
    }
}

// MARK: - Review extraction

private extension ProductsSignConfirmModelFactory {
    func payloadCall(from review: SignPayloadReview) -> (call: Data, genesisHash: Data) {
        switch review {
        case let .product(request): (request.payload.method, request.payload.genesisHash)
        case let .legacyAccount(request): (request.payload.method, request.payload.genesisHash)
        }
    }

    func transactionCall(from review: CreateTransactionReview) -> (call: Data, genesisHash: Data) {
        switch review {
        case let .product(payload): (payload.callData, payload.genesisHash)
        case let .legacyAccount(payload): (payload.callData, payload.genesisHash)
        }
    }

    func rawPayload(from review: SignRawReview) -> (payload: RawPayload, watermarked: Bool) {
        switch review {
        case let .product(request, watermarked): (request.payload, watermarked)
        case let .legacyAccount(request, watermarked): (request.payload, watermarked)
        }
    }
}
