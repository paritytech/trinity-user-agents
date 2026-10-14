import Foundation
import Products
import SubstrateSdk
import KeyDerivation
import BigInt
import ChainRegistry
import SubstrateSdkExt

protocol PolkadotSigningRequestResultMaking {
    func makeParsedResult(
        signingContext: PolkadotSigningRequestProviding
    ) async throws -> PolkadotParsedSigningRequestResult
}

final class PolkadotSigningRequestResultFactory {
    private let chainRegistry: ChainRegistryProtocol
    private let jsonPrinter: JSONPrettyPrinting
    private let extensionResolver: CreateTransactionExtensionResolver
    private let renderer: PolkadotSigningCallRenderer

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        jsonPrinter: JSONPrettyPrinting = ExtrinsicJSONProcessor(),
        extensionResolver: CreateTransactionExtensionResolver = CreateTransactionExtensionResolver()
    ) {
        self.chainRegistry = chainRegistry
        self.jsonPrinter = jsonPrinter
        self.extensionResolver = extensionResolver
        renderer = PolkadotSigningCallRenderer(jsonPrinter: jsonPrinter)
    }
}

extension PolkadotSigningRequestResultFactory: PolkadotSigningRequestResultMaking {
    func makeParsedResult(
        signingContext: PolkadotSigningRequestProviding
    ) async throws -> PolkadotParsedSigningRequestResult {
        try await parseRequest(signingContext: signingContext)
    }
}

private extension PolkadotSigningRequestResultFactory {
    func parseRequest(
        signingContext: PolkadotSigningRequestProviding
    ) async throws -> PolkadotParsedSigningRequestResult {
        let requester = signingContext.requester

        return switch signingContext.signingModel {
        case let .signingRequest(request):
            try await makeParsedSigningRequest(
                request: request,
                requester: requester,
                signingContext: signingContext
            )
        case let .createTransaction(payload):
            try await makeParsedCreateTransaction(
                payload: payload,
                wallet: signingContext.resolveWallet(for: payload.signer),
                requester: requester
            )
        case let .legacyRawPayload(account, type):
            try makeParsedRawDataResult(
                wallet: signingContext.resolveWallet(for: account),
                type: type,
                requester: requester
            )
        case let .legacyCreateTransaction(payload):
            try await makeParsedCreateTransaction(
                payload: payload,
                wallet: signingContext.resolveWallet(for: payload.signer.accountId),
                requester: requester
            )
        case let .legacySignPayload(payload):
            try await makeParsedLegacyTransaction(
                transaction: payload,
                wallet: signingContext.resolveWallet(for: payload.account.accountId),
                requester: requester
            )
        }
    }

    func makeParsedSigningRequest(
        request: PolkadotHostRemoteMessage.SigningRequest,
        requester: PolkadotSigningRequester,
        signingContext: PolkadotSigningRequestProviding
    ) async throws -> PolkadotParsedSigningRequestResult {
        switch request {
        case let .transaction(transaction):
            try await makeParsedLegacyTransaction(
                transaction: transaction,
                wallet: signingContext.resolveWallet(for: transaction.account),
                requester: requester
            )
        case let .rawPayload(rawPayload):
            try makeParsedRawDataResult(
                wallet: signingContext.resolveWallet(for: rawPayload.account),
                type: rawPayload.type,
                requester: requester
            )
        }
    }

    func makeParsedLegacyTransaction(
        transaction: SignTransactionPayload<some Any>,
        wallet: WalletManaging,
        requester: PolkadotSigningRequester
    ) async throws -> PolkadotParsedSigningRequestResult {
        let genesisHash = transaction.genesisHash.toHex()

        guard let chain = chainRegistry.getChainByGenesis(for: genesisHash) else {
            throw PolkadotSigningError.missingChain
        }

        guard
            let runtimeProvider = chainRegistry.getRuntimeProvider(for: chain.chainId),
            let codingFactory = try? await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        else {
            throw PolkadotSigningError.missingRuntimeProvider
        }

        let call = makeCall(
            codingFactory: codingFactory,
            transaction: transaction
        )

        let displayAddress = try makeDisplayAddress(for: wallet)

        let parsedTransaction = try PolkadotLegacyTransaction(
            address: displayAddress,
            blockHash: transaction.blockHash.toHex(),
            blockNumber: makeNumber(hexString: transaction.blockNumber.toHex()),
            era: makeEra(transaction: transaction),
            genesisHash: genesisHash,
            call: call,
            nonce: UInt32(makeNumber(hexString: transaction.nonce.toHex())),
            specVersion: UInt32(makeNumber(hexString: transaction.specVersion.toHex())),
            tip: makeNumber(hexString: transaction.tip.toHex()),
            transactionVersion: UInt32(makeNumber(hexString: transaction.transactionVersion.toHex())),
            metadataHash: transaction.metadataHash,
            assetId: nil,
            withSignedTransaction: transaction.withSignedTransaction ?? false,
            signedExtensions: transaction.signedExtensions,
            version: makeVersion(transaction: transaction)
        )

        let detailsText = try makeDetailsText(
            parsedTransaction: parsedTransaction,
            codingFactory: codingFactory
        )

        return PolkadotParsedSigningRequestResult(
            wallet: wallet,
            parsedRequest: .legacyTransaction(parsedTransaction),
            requester: requester,
            detailsText: detailsText
        )
    }

    func makeDisplayAddress(for wallet: WalletManaging) throws -> AccountAddress {
        try wallet.getRawPublicKey().toAddress(using: .genericFormat)
    }

    func makeCall(
        codingFactory: RuntimeCoderFactoryProtocol,
        transaction: SignTransactionPayload<some Any>
    ) -> PolkadotParsedTransactionCall {
        renderer.parseCall(from: transaction.method, codingFactory: codingFactory)
    }

    func makeEra(
        transaction: SignTransactionPayload<some Any>
    ) throws -> Era {
        let decoder = try ScaleDecoder(data: transaction.era)
        return try Era(scaleDecoder: decoder)
    }

    func makeVersion(
        transaction: SignTransactionPayload<some Any>
    ) throws -> PolkadotLegacyTransaction.Version {
        switch transaction.version {
        case 4: return .version4
        case 5: return .version5
        default: throw PolkadotSigningError.invalidVersion
        }
    }

    func makeNumber(hexString: String) throws -> BigUInt {
        guard let number = BigUInt.fromHexString(hexString) else {
            throw PolkadotSigningError.invalidNumberInHex
        }
        return number
    }

    func makeDetailsText(
        parsedTransaction: PolkadotLegacyTransaction,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> String {
        let detailsJSON = try parsedTransaction.toScaleCompatibleJSON(
            with: codingFactory.createRuntimeJsonContext().toRawContext()
        )
        return try jsonPrinter.prettyPrintedString(from: detailsJSON)
    }

    func makeParsedCreateTransaction(
        payload: CreateTransactionPayload<some Any>,
        wallet: WalletManaging,
        requester: PolkadotSigningRequester
    ) async throws -> PolkadotParsedSigningRequestResult {
        let callData = payload.callData
        let genesisHash = payload.genesisHash.toHex()

        guard let chain = chainRegistry.getChainByGenesis(for: genesisHash) else {
            throw PolkadotSigningError.missingChain
        }

        guard
            let runtimeProvider = chainRegistry.getRuntimeProvider(for: chain.chainId),
            let codingFactory = try? await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        else {
            throw PolkadotSigningError.missingRuntimeProvider
        }

        let resolved = try extensionResolver.resolve(
            extensions: payload.extensions,
            extensionVersion: payload.txExtVersion,
            codingFactory: codingFactory
        )

        let call = makeCall(codingFactory: codingFactory, callData: callData)

        let parsedCreateTx = PolkadotParsedCreateTransaction(
            callData: callData,
            call: call,
            resolvedExtensions: resolved,
            genesisHash: genesisHash,
            txExtVersion: payload.txExtVersion
        )

        let detailsText = try makeCreateTransactionDetailsText(
            call: call,
            codingFactory: codingFactory
        )

        return PolkadotParsedSigningRequestResult(
            wallet: wallet,
            parsedRequest: .createTransaction(parsedCreateTx),
            requester: requester,
            detailsText: detailsText
        )
    }

    func makeCall(
        codingFactory: RuntimeCoderFactoryProtocol,
        callData: Data
    ) -> PolkadotParsedTransactionCall {
        renderer.parseCall(from: callData, codingFactory: codingFactory)
    }

    func makeCreateTransactionDetailsText(
        call: PolkadotParsedTransactionCall,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> String {
        try renderer.callDetailsText(call, codingFactory: codingFactory)
    }

    func makeParsedRawDataResult(
        wallet: WalletManaging,
        type: PolkadotHostRemoteMessage.SigningRawPayload.PayloadType,
        requester: PolkadotSigningRequester
    ) throws -> PolkadotParsedSigningRequestResult {
        let rawBytes = try makeRawBytes(type: type)
        let detailsText = rawBytes.toHex(includePrefix: true)

        return PolkadotParsedSigningRequestResult(
            wallet: wallet,
            parsedRequest: .rawBytes(rawBytes),
            requester: requester,
            detailsText: detailsText
        )
    }

    func makeRawBytes(
        type: PolkadotHostRemoteMessage.SigningRawPayload.PayloadType
    ) throws -> Data {
        switch type {
        case let .bytes(data):
            try renderer.wrappedBytes(data)
        case let .payload(string):
            try renderer.wrappedBytes(fromString: string)
        }
    }
}
