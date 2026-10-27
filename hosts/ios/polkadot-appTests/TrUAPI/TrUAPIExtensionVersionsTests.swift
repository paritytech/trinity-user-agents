import Foundation
import Testing
import SubstrateSdk
import ChainRegistry
import TrUAPIHost
@testable import polkadot_app

@Suite("TrUAPIExtensionVersions")
struct TrUAPIExtensionVersionsTests {
    private let chainRegistry = MockChainRegistry()

    private func makeVersionProvider(versions: [ChainModel.Id: UInt8]) -> ExtrinsicVersionProvider {
        let config = MockRemoteConfigManager()
        config.txExtensionVersions = versions
        return ExtrinsicVersionProvider(
            extensionVersionProvider: ExtrinsicExtensionVersionProvider(remoteConfig: config),
            pipelineProvider: RuntimeTransactionExtensionPipelineProvider(chainRegistry: chainRegistry)
        )
    }

    private func registerChain(_ chainId: ChainModel.Id) throws -> Data {
        let genesis = try Data.randomOrError(of: 32)
        let remote = RemoteChainModel(
            chainId: chainId,
            parentId: nil,
            name: "TestChain",
            assets: [ChainMock.makeRemoteAsset()],
            nodes: [RemoteChainNodeModel(url: "wss://test.example.com", name: "Node", features: nil)],
            nodeSelectionStrategy: nil,
            addressPrefix: 42,
            genesisHash: genesis.toHex(),
            types: nil,
            icon: nil,
            options: nil,
            externalApi: nil,
            explorers: nil,
            additional: nil
        )
        chainRegistry.chainsById[chainId] = ChainMock.makeChainModel(from: remote, order: 0)
        return genesis
    }

    private func versionsByGenesis(_ map: ExtensionVersionMap) -> [Data: UInt8] {
        let pairs = map.chains.map { ($0.genesisHash, $0.extensionVersion) }
        return Dictionary(uniqueKeysWithValues: pairs)
    }

    @Test("general transaction chains are keyed by genesis hash; other chains are left to V4")
    func listsGeneralTransactionChains() throws {
        let people = try registerChain(AppConfig.Chains.peopleChain)
        let assetHub = try registerChain(AppConfig.Chains.assethubChain)
        _ = try registerChain(AppConfig.Chains.bulletInChain)
        let versionProvider = makeVersionProvider(versions: [AppConfig.Chains.peopleChain: 3])

        let map = try TrUAPIExtensionVersions.make(versionProvider: versionProvider, chainRegistry: chainRegistry)

        #expect(versionsByGenesis(map) == [people: 3, assetHub: 0])
    }

    @Test("a chain the registry cannot resolve fails the map instead of dropping to V4")
    func unresolvedChainFails() throws {
        _ = try registerChain(AppConfig.Chains.peopleChain)
        let versionProvider = makeVersionProvider(versions: [:])

        #expect(throws: TrUAPIExtensionVersions.Error.unresolvedChain(AppConfig.Chains.assethubChain)) {
            try TrUAPIExtensionVersions.make(versionProvider: versionProvider, chainRegistry: chainRegistry)
        }
    }
}
