import Foundation
import SubstrateSdk

public struct RemoteAssetModel: Equatable, Codable, Hashable {
    // swiftlint:disable:next type_name
    public typealias Id = UInt64
    public typealias PriceId = String

    public let assetId: Id
    public let icon: URL?
    public let name: String?
    public let symbol: String
    public let precision: UInt16
    public let priceId: PriceId?
    public let staking: [String]?
    public let type: String?
    public let typeExtras: JSON?
    public let buyProviders: JSON?

    public init(
        assetId: Id,
        icon: URL?,
        name: String?,
        symbol: String,
        precision: UInt16,
        priceId: PriceId?,
        staking: [String]?,
        type: String?,
        typeExtras: JSON?,
        buyProviders: JSON?
    ) {
        self.assetId = assetId
        self.icon = icon
        self.name = name
        self.symbol = symbol
        self.precision = precision
        self.priceId = priceId
        self.staking = staking
        self.type = type
        self.typeExtras = typeExtras
        self.buyProviders = buyProviders
    }
}
