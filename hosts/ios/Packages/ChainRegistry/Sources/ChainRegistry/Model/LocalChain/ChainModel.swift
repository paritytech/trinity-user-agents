import BigInt
import Foundation
import Operation_iOS
import SubstrateSdk

public struct ChainModel: Equatable, Hashable {
    // swiftlint:disable:next type_name
    public typealias Id = String

    public typealias AddressPrefix = UInt16

    public struct TypesSettings: Codable, Hashable {
        public let url: URL?
        public let overridesCommon: Bool

        public init(url: URL?, overridesCommon: Bool) {
            self.url = url
            self.overridesCommon = overridesCommon
        }
    }

    public struct Explorer: Codable, Hashable {
        public let name: String
        public let account: String?
        public let extrinsic: String?
        public let event: String?

        public init(name: String, account: String?, extrinsic: String?, event: String?) {
            self.name = name
            self.account = account
            self.extrinsic = extrinsic
            self.event = event
        }
    }

    public enum TypesUsage {
        case onlyCommon
        case both
        case onlyOwn
        case none
    }

    public enum NodeSwitchStrategy: String, Codable, Hashable {
        case uniform
        case roundRobin

        public init(rawStrategy: String?) {
            self = rawStrategy.flatMap { .init(rawValue: $0) } ?? .roundRobin
        }
    }

    public let chainId: Id
    public let parentId: Id?
    public let name: String
    public let assets: Set<AssetModel>
    public let nodes: Set<ChainNodeModel>
    public let addressPrefix: AddressPrefix
    public let explicitGenesisHash: String?
    public let types: TypesSettings?
    public let icon: URL?
    public let options: [LocalChainOptions]?
    public let externalApis: LocalChainExternalApiSet?
    public let nodeSwitchStrategy: NodeSwitchStrategy
    public let explorers: [Explorer]?
    public let order: Int64
    public let additional: JSON?
    public let syncMode: ChainSyncMode

    public init(
        chainId: Id,
        parentId: Id?,
        name: String,
        assets: Set<AssetModel>,
        nodes: Set<ChainNodeModel>,
        nodeSwitchStrategy: NodeSwitchStrategy,
        addressPrefix: UInt16,
        explicitGenesisHash: String?,
        types: TypesSettings?,
        icon: URL?,
        options: [LocalChainOptions]?,
        externalApis: LocalChainExternalApiSet?,
        explorers: [Explorer]?,
        order: Int64,
        additional: JSON?,
        syncMode: ChainSyncMode
    ) {
        self.chainId = chainId
        self.parentId = parentId
        self.name = name
        self.assets = assets
        self.nodes = nodes
        self.nodeSwitchStrategy = nodeSwitchStrategy
        self.addressPrefix = addressPrefix
        self.explicitGenesisHash = explicitGenesisHash
        self.types = types
        self.icon = icon
        self.options = options
        self.externalApis = externalApis
        self.explorers = explorers
        self.order = order
        self.additional = additional
        self.syncMode = syncMode
    }

    public init(remoteModel: RemoteChainModel, assets: Set<AssetModel>, syncMode: ChainSyncMode, order: Int64) {
        chainId = remoteModel.chainId
        parentId = remoteModel.parentId
        name = remoteModel.name
        self.assets = assets
        self.syncMode = syncMode

        let nodeList = remoteModel.nodes.enumerated().map { index, node in
            ChainNodeModel(remoteModel: node, order: Int16(index))
        }

        nodes = Set(nodeList)

        nodeSwitchStrategy = .init(rawStrategy: remoteModel.nodeSelectionStrategy)

        addressPrefix = remoteModel.addressPrefix
        explicitGenesisHash = remoteModel.genesisHash
        types = remoteModel.types
        icon = remoteModel.icon

        let remoteOptions = remoteModel.options?.compactMap { LocalChainOptions(rawValue: $0) } ?? []
        options = !remoteOptions.isEmpty ? remoteOptions : nil

        externalApis = remoteModel.externalApi.map { LocalChainExternalApiSet(remoteApi: $0) }
        explorers = remoteModel.explorers
        additional = remoteModel.additional

        self.order = order
    }

    public func asset(for assetId: AssetModel.Id) -> AssetModel? {
        assets.first { $0.assetId == assetId }
    }

    public func assetOrNil(for assetId: AssetModel.Id?) -> AssetModel? {
        guard let assetId else {
            return nil
        }

        return assets.first { $0.assetId == assetId }
    }

    public func assetOrNative(for assetId: AssetModel.Id?) -> AssetModel? {
        guard let assetId else {
            return utilityAsset()
        }

        return assets.first { $0.assetId == assetId }
    }

    public func hasEnabledAsset() -> Bool {
        assets.contains { $0.enabled }
    }

    public var isEthereumBased: Bool {
        options?.contains(.ethereumBased) ?? false
    }

    public var isTestnet: Bool {
        options?.contains(.testnet) ?? false
    }

    public var hasCrowdloans: Bool {
        options?.contains(.crowdloans) ?? false
    }

    public var hasGovernance: Bool {
        options?.contains(where: { $0 == .governance || $0 == .governanceV1 }) ?? false
    }

    public var hasGovernanceV1: Bool {
        options?.contains(where: { $0 == .governanceV1 }) ?? false
    }

    public var hasGovernanceV2: Bool {
        options?.contains(where: { $0 == .governance }) ?? false
    }

    public var hasSwapHub: Bool {
        options?.contains(where: { $0 == .swapHub }) ?? false
    }

    public var hasSwapHydra: Bool {
        options?.contains(where: { $0 == .swapHydra }) ?? false
    }

    public var hasSwaps: Bool {
        hasSwapHub || hasSwapHydra
    }

    public var hasProxy: Bool {
        options?.contains(where: { $0 == .proxy }) ?? false
    }

    public var noSubstrateRuntime: Bool {
        options?.contains(where: { $0 == .noSubstrateRuntime }) ?? false
    }

    public var hasSubstrateRuntime: Bool {
        !noSubstrateRuntime
    }

    public func chainAssetsWithExternalBalances() -> [ChainAsset] {
        assets.compactMap { asset in
            guard asset.hasPoolStaking || asset.isUtility && hasCrowdloans else {
                return nil
            }

            return ChainAsset(chain: self, asset: asset)
        }
    }

    public func chainAssetIdsWithExternalBalances() -> Set<ChainAssetId> {
        let chainAssets = chainAssetsWithExternalBalances()

        return Set(chainAssets.map(\.chainAssetId))
    }

    public var isRelaychain: Bool { parentId == nil }

    public func utilityAssets() -> Set<AssetModel> {
        assets.filter(\.isUtility)
    }

    public func utilityAsset() -> AssetModel? {
        utilityAssets().first
    }

    public func utilityChainAssetId() -> ChainAssetId? {
        guard let utilityAsset = utilityAssets().first else {
            return nil
        }

        return ChainAssetId(chainId: chainId, assetId: utilityAsset.assetId)
    }

    public func utilityChainAsset() -> ChainAsset? {
        guard let utilityAsset = utilityAssets().first else {
            return nil
        }

        return ChainAsset(chain: self, asset: utilityAsset)
    }

    public func chainAssets() -> [ChainAsset] {
        assets.map { ChainAsset(chain: self, asset: $0) }
    }

    public var typesUsage: TypesUsage {
        guard let types else {
            return .none
        }

        guard !types.overridesCommon else {
            return .onlyOwn
        }

        return types.url != nil ? .both : .onlyCommon
    }

    public var defaultTip: BigUInt? {
        if let tipString = additional?.defaultTip?.stringValue {
            BigUInt(tipString)
        } else {
            nil
        }
    }

    public var defaultBlockTimeMillis: BlockTime? {
        additional?.defaultBlockTime?.unsignedIntValue
    }

    public var isUtilityTokenOnRelaychain: Bool {
        additional?.relaychainAsNative?.boolValue ?? false
    }

    public var stakingMaxElectingVoters: UInt32? {
        guard let value = additional?.stakingMaxElectingVoters?.unsignedIntValue else {
            return nil
        }

        return UInt32(value)
    }

    public var isDisabled: Bool {
        syncMode == .disabled
    }

    public var isFullSyncMode: Bool {
        syncMode == .full
    }

    public var isLightSyncMode: Bool {
        syncMode == .light
    }

    public var feeViaRuntimeCall: Bool {
        additional?.feeViaRuntimeCall?.boolValue ?? false
    }
}

extension ChainModel: Identifiable {
    public var identifier: String { chainId }
}

public enum LocalChainOptions: String, Codable {
    case ethereumBased
    case testnet
    case crowdloans
    case governance
    case governanceV1 = "governance-v1"
    case noSubstrateRuntime
    case swapHub = "swap-hub"
    case swapHydra = "hydradx-swaps"
    case proxy
}

public extension ChainModel {
    func adding(asset: AssetModel) -> ChainModel {
        .init(
            chainId: chainId,
            parentId: parentId,
            name: name,
            assets: assets.union([asset]),
            nodes: nodes,
            nodeSwitchStrategy: nodeSwitchStrategy,
            addressPrefix: addressPrefix,
            explicitGenesisHash: explicitGenesisHash,
            types: types,
            icon: icon,
            options: options,
            externalApis: externalApis,
            explorers: explorers,
            order: order,
            additional: additional,
            syncMode: syncMode
        )
    }

    func addingOrUpdating(asset: AssetModel) -> ChainModel {
        let filteredAssets = assets.filter { $0.assetId != asset.assetId }
        let newAssets = filteredAssets.union([asset])

        return .init(
            chainId: chainId,
            parentId: parentId,
            name: name,
            assets: newAssets,
            nodes: nodes,
            nodeSwitchStrategy: nodeSwitchStrategy,
            addressPrefix: addressPrefix,
            explicitGenesisHash: explicitGenesisHash,
            types: types,
            icon: icon,
            options: options,
            externalApis: externalApis,
            explorers: explorers,
            order: order,
            additional: additional,
            syncMode: syncMode
        )
    }

    func byChanging(assets: Set<AssetModel>? = nil, name: String? = nil) -> ChainModel {
        let newAssets = assets ?? self.assets
        let newName = name ?? self.name

        return .init(
            chainId: chainId,
            parentId: parentId,
            name: newName,
            assets: newAssets,
            nodes: nodes,
            nodeSwitchStrategy: nodeSwitchStrategy,
            addressPrefix: addressPrefix,
            explicitGenesisHash: explicitGenesisHash,
            types: types,
            icon: icon,
            options: options,
            externalApis: externalApis,
            explorers: explorers,
            order: order,
            additional: additional,
            syncMode: syncMode
        )
    }

    func updatingSyncMode(for newMode: ChainSyncMode) -> ChainModel {
        .init(
            chainId: chainId,
            parentId: parentId,
            name: name,
            assets: assets,
            nodes: nodes,
            nodeSwitchStrategy: nodeSwitchStrategy,
            addressPrefix: addressPrefix,
            explicitGenesisHash: explicitGenesisHash,
            types: types,
            icon: icon,
            options: options,
            externalApis: externalApis,
            explorers: explorers,
            order: order,
            additional: additional,
            syncMode: newMode
        )
    }
}

public extension ChainModel {
    func getAllAssetPriceIds() -> Set<AssetModel.PriceId> {
        let priceIds = assets.compactMap(\.priceId)
        return Set(priceIds)
    }
}

public enum ChainModelError: Error {
    case missingAsset(AssetModel.Id)
}

public extension ChainModel {
    func chainAsset(for assetId: AssetModel.Id) -> ChainAsset? {
        guard let assetModel = assets.first(where: { $0.assetId == assetId }) else {
            return nil
        }

        return ChainAsset(chain: self, asset: assetModel)
    }

    func chainAssetOrError(for assetId: AssetModel.Id) throws -> ChainAsset {
        guard let asset = chainAsset(for: assetId) else {
            throw ChainModelError.missingAsset(assetId)
        }

        return asset
    }
}
