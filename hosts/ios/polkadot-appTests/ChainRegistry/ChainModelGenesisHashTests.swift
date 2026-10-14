import Foundation
import Testing
import SubstrateSdk
import ChainRegistry

@testable import polkadot_app

@Suite("ChainModel genesisHash override")
struct ChainModelGenesisHashTests {
    private func makeChain(chainId: String, storedGenesis: String?) -> ChainModel {
        let remote = RemoteChainModel(
            chainId: chainId,
            parentId: nil,
            name: "TestChain",
            assets: [ChainMock.makeRemoteAsset()],
            nodes: [RemoteChainNodeModel(url: "wss://test.example.com", name: "Node", features: nil)],
            nodeSelectionStrategy: nil,
            addressPrefix: 42,
            genesisHash: storedGenesis,
            types: nil,
            icon: nil,
            options: nil,
            externalApi: nil,
            explorers: nil,
            additional: nil
        )

        return ChainMock.makeChainModel(from: remote, order: 0)
    }

    @Test("returns the stored genesis hash when present")
    func returnsStoredGenesisWhenPresent() throws {
        let storedHex = try Data.randomOrError(of: 32).toHex()
        let chain = makeChain(chainId: "release-people", storedGenesis: storedHex)

        #expect(chain.explicitGenesisHash == storedHex)
        #expect(chain.genesisHash == storedHex)
    }

    @Test("falls back to chainId when no stored genesis hash")
    func fallsBackToChainIdWhenMissing() {
        let chain = makeChain(chainId: "release-people", storedGenesis: nil)

        #expect(chain.explicitGenesisHash == nil)
        #expect(chain.genesisHash == chain.chainId)
    }
}
