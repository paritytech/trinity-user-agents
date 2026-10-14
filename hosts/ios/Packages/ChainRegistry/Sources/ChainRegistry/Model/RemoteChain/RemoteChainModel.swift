import Foundation
import Operation_iOS
import SubstrateSdk

public struct RemoteChainModel: Equatable, Codable {
    public let chainId: ChainModel.Id
    public let parentId: ChainModel.Id?
    public let name: String
    public let assets: [RemoteAssetModel]
    public let nodes: [RemoteChainNodeModel]
    public let nodeSelectionStrategy: String?
    public let addressPrefix: UInt16
    public let genesisHash: String?
    public let types: ChainModel.TypesSettings?
    public let icon: URL?
    public let options: [String]?
    public let externalApi: RemoteChainExternalApiSet?
    public let explorers: [ChainModel.Explorer]?
    public let additional: JSON?

    public init(
        chainId: ChainModel.Id,
        parentId: ChainModel.Id?,
        name: String,
        assets: [RemoteAssetModel],
        nodes: [RemoteChainNodeModel],
        nodeSelectionStrategy: String?,
        addressPrefix: UInt16,
        genesisHash: String?,
        types: ChainModel.TypesSettings?,
        icon: URL?, options: [String]?,
        externalApi: RemoteChainExternalApiSet?,
        explorers: [ChainModel.Explorer]?,
        additional: JSON?
    ) {
        self.chainId = chainId
        self.parentId = parentId
        self.name = name
        self.assets = assets
        self.nodes = nodes
        self.nodeSelectionStrategy = nodeSelectionStrategy
        self.addressPrefix = addressPrefix
        self.genesisHash = genesisHash
        self.types = types
        self.icon = icon
        self.options = options
        self.externalApi = externalApi
        self.explorers = explorers
        self.additional = additional
    }
}

public extension RemoteChainModel {
    func byChanging(name: String) -> RemoteChainModel {
        .init(
            chainId: chainId,
            parentId: parentId,
            name: name,
            assets: assets,
            nodes: nodes,
            nodeSelectionStrategy: nodeSelectionStrategy,
            addressPrefix: addressPrefix,
            genesisHash: genesisHash,
            types: types,
            icon: icon,
            options: options,
            externalApi: externalApi,
            explorers: explorers,
            additional: additional
        )
    }
}

public enum RemoteOnlyChainOptions: String {
    case fullSyncByDefault
}
