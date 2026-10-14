import Foundation
import Keystore_iOS
import ExtrinsicService
import SubstrateSdk
import SubstrateSdkExt
import Individuality
import KeyDerivation
import ChainRegistry

protocol ExtrinsicOriginFactoryProtocol {
    func createSignedOrigin(
        for wallet: WalletManaging,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining
}

enum ExtrinsicOriginFactoryError: Error {
    case unexpectedWalletModel
}

class ExtrinsicOriginFactory {
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ExtrinsicOriginFactory: ExtrinsicOriginFactoryProtocol {
    func createSignedOrigin(
        for wallet: WalletManaging,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining {
        let accountOrigin = try createAccountOrigin(for: wallet, chain: chain)

        let signedOrigin = try createSigningByAccountOrigin(for: wallet, chain: chain)

        let restrictionOrigin = RestrictsOriginDefinition(enabled: false)

        return ExtrinsicCompoundOrigin(children: [accountOrigin, restrictionOrigin, signedOrigin])
    }
}

extension ExtrinsicOriginFactory {
    func createAccountOrigin(
        for wallet: MetaAccountModelProtocol,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining {
        let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

        let account = try wallet.fetchAccount(for: chain)

        let senderResolvingFactory = ExtrinsicSenderResolutionFactory(account: account)

        return ExtrinsicAccountOrigin(
            connection: connection,
            runtimeProvider: runtimeProvider,
            senderResolvingFactory: senderResolvingFactory,
            nonceOperationFactory: SubstrateNonceOperationFactory(
                chain: chain,
                connection: connection,
                timeout: JSONRPCTimeout.singleNode
            )
        )
    }

    func createSigningByAccountOrigin(
        for wallet: WalletManaging,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining {
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

        let signingWrapperFactory = DefaultSigningWrapperFactory(secretProvider: wallet)

        return ExtrinsicSignedOrigin(runtimeProvider: runtimeProvider, signingWrapperFactory: signingWrapperFactory)
    }

    func createFeeModifier(for chain: ChainProtocol) throws -> ExtrinsicOriginDefining {
        let runtimeService = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

        return ExtrinsicFeePaymentModifier(runtimeService: runtimeService, operationQueue: operationQueue)
    }
}
