import Foundation
import SubstrateSdk
import ChainRegistry

@testable import polkadot_app

enum ChainMock {
    static func randomChainId() -> String {
        Data.random(of: 32)!.toHex()
    }

    static func makeRemoteChain(
        id: String? = nil,
        name: String,
        options: [String]? = nil
    ) -> RemoteChainModel {
        let chainId = id ?? randomChainId()
        return RemoteChainModel(
            chainId: chainId,
            parentId: nil,
            name: name,
            assets: [makeRemoteAsset()],
            nodes: [RemoteChainNodeModel(url: "wss://\(chainId).example.com", name: "Node", features: nil)],
            nodeSelectionStrategy: nil,
            addressPrefix: 42,
            genesisHash: nil,
            types: nil,
            icon: nil,
            options: options,
            externalApi: nil,
            explorers: nil,
            additional: nil
        )
    }

    static func makeRemoteAsset(id: AssetModel.Id = 0) -> RemoteAssetModel {
        RemoteAssetModel(
            assetId: id,
            icon: nil,
            name: "DOT",
            symbol: "DOT",
            precision: 10,
            priceId: nil,
            staking: nil,
            type: nil,
            typeExtras: nil,
            buyProviders: nil
        )
    }

    static func makeChainModel(
        from remote: RemoteChainModel,
        order: Int64,
        syncMode: ChainSyncMode = .full
    ) -> ChainModel {
        ChainModel(
            remoteModel: remote,
            assets: Set(remote.assets.map { AssetModel(remoteModel: $0, enabled: true) }),
            syncMode: syncMode,
            order: order
        )
    }
}
