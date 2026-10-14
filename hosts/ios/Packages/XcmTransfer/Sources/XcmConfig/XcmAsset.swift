import Foundation
import SubstrateSdk

public struct XcmAsset: Decodable {
    public let assetId: AssetId
    public let assetLocation: String
    public let assetLocationPath: XcmAsset.Location
    public let xcmTransfers: [XcmAssetTransfer]

    public init(
        assetId: AssetId,
        assetLocation: String,
        assetLocationPath: XcmAsset.Location,
        xcmTransfers: [XcmAssetTransfer]
    ) {
        self.assetId = assetId
        self.assetLocation = assetLocation
        self.assetLocationPath = assetLocationPath
        self.xcmTransfers = xcmTransfers
    }
}

public extension XcmAsset {
    enum LocationType: String, Decodable {
        case absolute
        case relative
        case concrete
    }

    struct Location: Decodable {
        public let type: LocationType
        public let path: JSON?

        public init(type: LocationType, path: JSON?) {
            self.type = type
            self.path = path
        }
    }

    struct ReservePath {
        public let type: LocationType
        public let path: JSON

        public init(type: LocationType, path: JSON) {
            self.type = type
            self.path = path
        }
    }
}
