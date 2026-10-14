import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

extension ChainModelMapper: CoreDataMapperProtocol {
    func transform(entity: CDChain) throws -> ChainModel {
        let assets: [AssetModel] = try entity.assets?.compactMap { anyAsset in
            guard let asset = anyAsset as? CDAsset else {
                return nil
            }

            return try createAsset(from: asset)
        } ?? []

        let nodes: [ChainNodeModel] = try entity.nodes?.compactMap { anyNode in
            guard let node = anyNode as? CDChainNodeItem else {
                return nil
            }

            return try createChainNode(from: node)
        } ?? []

        let nodeSwitchStrategy = ChainModel.NodeSwitchStrategy(rawStrategy: entity.nodeSwitchStrategy)

        let types: ChainModel.TypesSettings? =
            if entity.types != nil || entity.typesOverrideCommon != nil {
                .init(url: entity.types, overridesCommon: entity.typesOverrideCommon?.boolValue ?? false)
            } else {
                nil
            }

        let externalApiSet = createExternalApis(from: entity.externalApis)
        let explorers = createExplorers(from: entity)

        let options = createChainOptions(from: entity)

        let additional: JSON? = try entity.additional.map {
            try jsonDecoder.decode(JSON.self, from: $0)
        }

        guard let syncMode = ChainSyncMode(entityValue: entity.syncMode) else {
            throw ChainModelMapperError.unexpectedSyncMode(entity.syncMode)
        }

        return ChainModel(
            chainId: entity.chainId!,
            parentId: entity.parentId,
            name: entity.name!,
            assets: Set(assets),
            nodes: Set(nodes),
            nodeSwitchStrategy: nodeSwitchStrategy,
            addressPrefix: UInt16(bitPattern: entity.addressPrefix),
            explicitGenesisHash: entity.genesisHash,
            types: types,
            icon: entity.icon,
            options: options,
            externalApis: externalApiSet,
            explorers: explorers,
            order: entity.order,
            additional: additional,
            syncMode: syncMode
        )
    }

    func populate(
        entity: CDChain,
        from model: ChainModel,
        using context: NSManagedObjectContext
    ) throws {
        entity.chainId = model.chainId
        entity.parentId = model.parentId
        entity.name = model.name
        entity.types = model.types?.url
        entity.typesOverrideCommon = model.types.map { NSNumber(value: $0.overridesCommon) }

        entity.addressPrefix = Int16(bitPattern: model.addressPrefix)
        entity.icon = model.icon
        entity.isEthereumBased = model.isEthereumBased
        entity.isTestnet = model.isTestnet
        entity.hasCrowdloans = model.hasCrowdloans
        entity.hasGovernanceV1 = model.hasGovernanceV1
        entity.hasGovernance = model.hasGovernanceV2
        entity.noSubstrateRuntime = model.noSubstrateRuntime
        entity.genesisHash = model.explicitGenesisHash
        entity.hasSwapHub = model.hasSwapHub
        entity.hasSwapHydra = model.hasSwapHydra
        entity.hasProxy = model.hasProxy
        entity.order = model.order
        entity.nodeSwitchStrategy = model.nodeSwitchStrategy.rawValue
        entity.additional = try model.additional.map {
            try jsonEncoder.encode($0)
        }

        entity.syncMode = model.syncMode.toEntityValue()

        try updateEntityAssets(for: entity, from: model, context: context)

        try updateEntityNodes(for: entity, from: model, context: context)

        try updateExternalApis(for: entity, from: model, context: context)

        updateExplorers(for: entity, from: model.explorers)
    }
}
