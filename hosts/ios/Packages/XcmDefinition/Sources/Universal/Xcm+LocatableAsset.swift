import Foundation

public extension XcmUni {
    struct LocatableAsset: Equatable {
        public let location: RelativeLocation
        public let assetId: AssetId

        public init(location: RelativeLocation, assetId: AssetId) {
            self.location = location
            self.assetId = assetId
        }
    }
}

extension XcmUni.LocatableAsset: XcmUniCodable {
    enum CodingKeys: String, CodingKey {
        case location
        case assetId
    }

    public init(from decoder: any Decoder, configuration: Xcm.Version) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        location = try container.decode(
            XcmUni.RelativeLocation.self,
            forKey: .location,
            configuration: configuration
        )

        assetId = try container.decode(
            XcmUni.AssetId.self,
            forKey: .assetId,
            configuration: configuration
        )
    }

    public func encode(to encoder: any Encoder, configuration: Xcm.Version) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        try container.encode(location, forKey: .location, configuration: configuration)
        try container.encode(assetId, forKey: .assetId, configuration: configuration)
    }
}
