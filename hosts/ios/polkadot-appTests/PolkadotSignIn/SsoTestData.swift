import Foundation
import KeyDerivation
import Keystore_iOS
import Operation_iOS
import Products
import SubstrateSdk
import ChainRegistry

@testable import polkadot_app

enum SsoTestData {
    static let entropyManager: RootEntropyManaging = {
        let keychain = InMemoryKeychain()
        let store = MockInstallationKeyIdStore()
        let manager = RootEntropyManager(keychain: keychain, installationKeyIdStore: store)
        let entropy = Data.random(of: 32)!
        try! manager.createRootEntropy(entropy)
        return manager
    }()

    static func makeWallet(derivationPath: String = "//wallet") -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: derivationPath, entropyManager: entropyManager)
    }

    static func makeRequester(name: String = "TestDApp") -> PolkadotSigningRequester {
        PolkadotSigningRequester(name: name, iconUrl: nil)
    }

    static func makeChain(genesisHash: String) -> ChainModel {
        let remote = RemoteChainModel(
            chainId: ChainMock.randomChainId(),
            parentId: nil,
            name: "TestChain",
            assets: [ChainMock.makeRemoteAsset()],
            nodes: [RemoteChainNodeModel(url: "wss://test.example.com", name: "Node", features: nil)],
            nodeSelectionStrategy: nil,
            addressPrefix: 42,
            genesisHash: genesisHash,
            types: nil,
            icon: nil,
            options: nil,
            externalApi: nil,
            explorers: nil,
            additional: nil
        )
        return ChainMock.makeChainModel(from: remote, order: 0)
    }

    static func makeAccount(productId: String = "test.product", derivationIndex: UInt32 = 0) -> ProductAccountId {
        ProductAccountId(productId: productId, derivationIndex: .index(derivationIndex))
    }

    static func makeTransaction(
        account: ProductAccountId,
        genesisHash: Data
    ) -> SignTransactionPayload<ProductAccountId> {
        SignTransactionPayload(
            account: account,
            blockHash: Data(repeating: 0, count: 32),
            blockNumber: Data([0x00]),
            era: Data([0x00]),
            genesisHash: genesisHash,
            method: Data([0x00, 0x01]),
            nonce: Data([0x00]),
            specVersion: Data([0x01]),
            tip: Data([0x00]),
            transactionVersion: Data([0x01]),
            signedExtensions: [],
            version: 4,
            assetId: nil,
            metadataHash: nil,
            mode: nil,
            withSignedTransaction: nil
        )
    }

    static func makeSigningContext(
        wallet: WalletManaging,
        signingRequest: PolkadotHostRemoteMessage.SigningRequest,
        requester: PolkadotSigningRequester = makeRequester()
    ) -> MockSigningContext {
        MockSigningContext(
            requester: requester,
            signingModel: .signingRequest(signingRequest),
            wallet: wallet
        )
    }

    static func makeFailingSigningContext(
        signingRequest: PolkadotHostRemoteMessage.SigningRequest,
        error: Error,
        requester: PolkadotSigningRequester = makeRequester()
    ) -> MockSigningContext {
        MockSigningContext(
            requester: requester,
            signingModel: .signingRequest(signingRequest),
            resolveError: error
        )
    }

    static func makeChainRegistry(
        chain: ChainModel,
        genesisHash: String,
        coderFactory: RuntimeCoderFactoryProtocol? = MockCoderFactory()
    ) -> MockChainRegistry {
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsByGenesis[genesisHash] = chain

        if let coderFactory {
            let runtimeProvider = MockRuntimeProvider()
            runtimeProvider.chainId = chain.chainId
            runtimeProvider.coderFactory = coderFactory
            chainRegistry.runtimeProviders[chain.chainId] = runtimeProvider
        }

        return chainRegistry
    }

    static func makeFactory(
        chainRegistry: ChainRegistryProtocol = MockChainRegistry()
    ) -> PolkadotSigningRequestResultFactory {
        PolkadotSigningRequestResultFactory(chainRegistry: chainRegistry)
    }
}
