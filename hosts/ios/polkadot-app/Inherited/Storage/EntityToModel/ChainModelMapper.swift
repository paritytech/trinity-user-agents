import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

enum ChainModelMapperError: Error {
    case unexpectedSyncMode(Int16)
}

final class ChainModelMapper {
    var entityIdentifierFieldName: String { #keyPath(CDChain.chainId) }

    typealias DataProviderModel = ChainModel
    typealias CoreDataEntity = CDChain

    private(set) lazy var jsonEncoder = JSONEncoder()
    private(set) lazy var jsonDecoder = JSONDecoder()

    func createNodeFeatures(from entityData: Data?) throws -> Set<ChainNodeModel.Feature>? {
        let featureList = try entityData.flatMap { try jsonDecoder.decode([String].self, from: $0) }

        return featureList.flatMap { Set($0.compactMap { .init(rawValue: $0) }) }
    }

    func serializeNodeFeature(from model: Set<ChainNodeModel.Feature>?) throws -> Data? {
        try model.flatMap { try jsonEncoder.encode($0) }
    }

    func createStakings(from entity: CDAsset) throws -> [StakingType]? {
        guard let staking = entity.staking else {
            return nil
        }

        let rawStakings = staking.split(by: String.Separator.comma)

        return rawStakings.map { StakingType(rawType: $0) }
    }

    func updateStakings(on entity: CDAsset, newStakings: [StakingType]?) throws {
        let rawStakings = newStakings?.map(\.rawValue).joined(with: String.Separator.comma)
        entity.staking = rawStakings
    }

    func createAsset(from entity: CDAsset) throws -> AssetModel {
        let typeExtras: JSON? =
            if let data = entity.typeExtras {
                try jsonDecoder.decode(JSON.self, from: data)
            } else {
                nil
            }

        let buyProviders: JSON? =
            if let data = entity.buyProviders {
                try jsonDecoder.decode(JSON.self, from: data)
            } else {
                nil
            }

        let source = AssetModel.Source(rawValue: entity.source!) ?? .remote

        let stakings = try createStakings(from: entity)

        return AssetModel(
            assetId: UInt64(bitPattern: entity.assetId),
            icon: entity.icon,
            name: entity.name,
            symbol: entity.symbol!,
            precision: UInt16(bitPattern: entity.precision),
            priceId: entity.priceId,
            stakings: stakings,
            type: entity.type,
            typeExtras: typeExtras,
            buyProviders: buyProviders,
            enabled: entity.enabled,
            source: source
        )
    }

    func createChainNode(from entity: CDChainNodeItem) throws -> ChainNodeModel {
        let features = try createNodeFeatures(from: entity.features)

        return ChainNodeModel(
            url: entity.url!,
            name: entity.name!,
            order: entity.order,
            features: features
        )
    }

    func updateEntityAssets(
        for entity: CDChain,
        from model: ChainModel,
        context: NSManagedObjectContext
    ) throws {
        let assetEntities: [CDAsset] = try model.assets.map { asset in
            let assetEntity: CDAsset
            let assetEntityId = Int64(bitPattern: asset.assetId)

            let maybeExistingEntity = entity.assets?
                .first { ($0 as? CDAsset)?.assetId == assetEntityId } as? CDAsset

            if let existingEntity = maybeExistingEntity {
                assetEntity = existingEntity
            } else {
                assetEntity = try context.insertNew(CDAsset.self)
            }

            assetEntity.assetId = assetEntityId
            assetEntity.name = asset.name
            assetEntity.precision = Int16(bitPattern: asset.precision)
            assetEntity.icon = asset.icon
            assetEntity.symbol = asset.symbol
            assetEntity.priceId = asset.priceId
            assetEntity.type = asset.type
            assetEntity.enabled = asset.enabled
            assetEntity.source = asset.source.rawValue

            try updateStakings(on: assetEntity, newStakings: asset.stakings)

            if let json = asset.typeExtras {
                assetEntity.typeExtras = try jsonEncoder.encode(json)
            } else {
                assetEntity.typeExtras = nil
            }

            if let json = asset.buyProviders {
                assetEntity.buyProviders = try jsonEncoder.encode(json)
            } else {
                assetEntity.buyProviders = nil
            }

            return assetEntity
        }

        let existingAssetIds = Set(model.assets.map(\.assetId))

        if let oldAssets = entity.assets as? Set<CDAsset> {
            for oldAsset in oldAssets where !existingAssetIds.contains(UInt64(bitPattern: oldAsset.assetId)) {
                context.delete(oldAsset)
            }
        }

        entity.assets = NSSet(array: assetEntities)
    }

    func updateEntityNodes(
        for entity: CDChain,
        from model: ChainModel,
        context: NSManagedObjectContext
    ) throws {
        let nodeEntities: [CDChainNodeItem] = try model.nodes.map { node in
            let nodeEntity: CDChainNodeItem

            let maybeExistingEntity = entity.nodes?
                .first { ($0 as? CDChainNodeItem)?.url == node.url } as? CDChainNodeItem

            if let existingEntity = maybeExistingEntity {
                nodeEntity = existingEntity
            } else {
                nodeEntity = try context.insertNew(CDChainNodeItem.self)
            }

            nodeEntity.url = node.url
            nodeEntity.name = node.name
            nodeEntity.order = node.order
            nodeEntity.features = try serializeNodeFeature(from: node.features)

            return nodeEntity
        }

        let existingNodeIds = Set(model.nodes.map(\.url))

        if let oldNodes = entity.nodes as? Set<CDChainNodeItem> {
            for oldNode in oldNodes where !existingNodeIds.contains(oldNode.url!) {
                context.delete(oldNode)
            }
        }

        entity.nodes = NSSet(array: nodeEntities)
    }

    func createExplorers(from chain: CDChain) -> [ChainModel.Explorer]? {
        guard let data = chain.explorers else {
            return nil
        }

        return try? JSONDecoder().decode([ChainModel.Explorer].self, from: data)
    }

    func updateExplorers(for entity: CDChain, from explorers: [ChainModel.Explorer]?) {
        if let explorers {
            entity.explorers = try? JSONEncoder().encode(explorers)
        } else {
            entity.explorers = nil
        }
    }

    func createExternalApis(from entityApis: NSSet?) -> LocalChainExternalApiSet? {
        guard let entityApis = entityApis as? Set<CDChainApi>, !entityApis.isEmpty else {
            return nil
        }

        let apis = entityApis.map {
            let parameters: JSON? =
                if let rawParameters = $0.parameters {
                    try? jsonDecoder.decode(JSON.self, from: rawParameters)
                } else {
                    nil
                }

            return LocalChainExternalApi(
                apiType: $0.apiType!,
                serviceType: $0.serviceType!,
                url: $0.url!,
                parameters: parameters
            )
        }

        return .init(localApis: Set(apis))
    }

    func updateExternalApis(
        for entity: CDChain,
        from model: ChainModel,
        context: NSManagedObjectContext
    ) throws {
        let optApiEntities: [CDChainApi]? = try model.externalApis?.apis.map { apiModel in
            let apiEntity: CDChainApi

            let maybeExistingEntity = entity.externalApis?.first { entity in
                guard let entity = entity as? CDChainApi else {
                    return false
                }

                return entity.identifier == apiModel.identifier
            } as? CDChainApi

            if let existingEntity = maybeExistingEntity {
                apiEntity = existingEntity
            } else {
                apiEntity = try context.insertNew(CDChainApi.self)
            }

            apiEntity.apiType = apiModel.apiType
            apiEntity.url = apiModel.url
            apiEntity.serviceType = apiModel.serviceType

            if let parameters = apiModel.parameters {
                apiEntity.parameters = try? jsonEncoder.encode(parameters)
            } else {
                apiEntity.parameters = nil
            }

            return apiEntity
        }

        let existingApiEntities = Set((model.externalApis?.apis ?? []).map(\.identifier))

        if let oldApis = entity.externalApis as? Set<CDChainApi> {
            for oldApi in oldApis where !existingApiEntities.contains(oldApi.identifier) {
                context.delete(oldApi)
            }
        }

        if let apiEntities = optApiEntities {
            entity.externalApis = NSSet(array: apiEntities)
        } else {
            entity.externalApis = nil
        }
    }

    func createChainOptions(from entity: CDChain) -> [LocalChainOptions]? {
        var options: [LocalChainOptions] = []

        if entity.isEthereumBased {
            options.append(.ethereumBased)
        }

        if entity.isTestnet {
            options.append(.testnet)
        }

        if entity.hasCrowdloans {
            options.append(.crowdloans)
        }

        if entity.hasGovernance {
            options.append(.governance)
        }

        if entity.hasGovernanceV1 {
            options.append(.governanceV1)
        }

        if entity.noSubstrateRuntime {
            options.append(.noSubstrateRuntime)
        }

        if entity.hasSwapHub {
            options.append(.swapHub)
        }

        if entity.hasSwapHydra {
            options.append(.swapHydra)
        }

        if entity.hasProxy {
            options.append(.proxy)
        }

        return !options.isEmpty ? options : nil
    }
}
