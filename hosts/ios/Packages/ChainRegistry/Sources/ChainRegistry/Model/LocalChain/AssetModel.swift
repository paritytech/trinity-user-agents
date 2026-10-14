import Foundation
import SubstrateSdk

public struct AssetModel: Equatable, Codable, Hashable {
    public enum Source: String, Codable {
        case remote
        case user
    }

    // swiftlint:disable:next type_name
    public typealias Id = UInt64
    public typealias PriceId = String

    public static let utilityAssetId: Id = 0

    public let assetId: Id
    public let icon: URL?
    public let name: String?
    public let symbol: String
    public let precision: UInt16
    public let priceId: PriceId?
    public let stakings: [StakingType]?
    public let type: String?
    public let typeExtras: JSON?
    public let buyProviders: JSON?

    // local properties
    public let enabled: Bool
    public let source: Source

    public var isUtility: Bool { assetId == Self.utilityAssetId }

    public var hasStaking: Bool {
        stakings?.contains { $0 != .unsupported } ?? false
    }

    public var hasPoolStaking: Bool {
        stakings?.contains { $0 == .nominationPools } ?? false
    }

    public init(
        assetId: Id,
        icon: URL?,
        name: String?,
        symbol: String,
        precision: UInt16,
        priceId: PriceId?,
        stakings: [StakingType]?,
        type: String?,
        typeExtras: JSON?,
        buyProviders: JSON?,
        enabled: Bool,
        source: Source
    ) {
        self.assetId = assetId
        self.icon = icon
        self.name = name
        self.symbol = symbol
        self.precision = precision
        self.priceId = priceId
        self.stakings = stakings
        self.type = type
        self.typeExtras = typeExtras
        self.buyProviders = buyProviders
        self.enabled = enabled
        self.source = source
    }

    public init(remoteModel: RemoteAssetModel, enabled: Bool) {
        assetId = remoteModel.assetId
        icon = remoteModel.icon
        name = remoteModel.name
        symbol = remoteModel.symbol
        precision = remoteModel.precision
        priceId = remoteModel.priceId
        stakings = remoteModel.staking?.map { StakingType(rawType: $0) }
        type = remoteModel.type
        typeExtras = remoteModel.typeExtras
        buyProviders = remoteModel.buyProviders
        self.enabled = enabled
        source = .remote
    }

    public var hasPrice: Bool {
        priceId != nil
    }
}

public extension AssetModel {
    var decimalPrecision: Int16 {
        Int16(bitPattern: precision)
    }
}

public extension AssetModel {
    func byChanging(enabled: Bool) -> AssetModel {
        .init(
            assetId: assetId,
            icon: icon,
            name: name,
            symbol: symbol,
            precision: precision,
            priceId: priceId,
            stakings: stakings,
            type: type,
            typeExtras: typeExtras,
            buyProviders: buyProviders,
            enabled: enabled,
            source: source
        )
    }
}

public extension AssetModel {
    var supportedStakings: [StakingType]? {
        stakings?.filter { $0 != .unsupported }
    }
}
