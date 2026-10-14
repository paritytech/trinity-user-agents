import Foundation
import SubstrateSdk
import BigInt
import XcmDefinition

public enum AssetConversionPallet {
    public static let name = "AssetConversion"

    public typealias AssetId = Xcm.Version4<XcmUni.AssetId>

    public enum PoolAsset {
        case native
        case assets(pallet: UInt8, index: BigUInt)
        case foreign(AssetId)
        case undefined(AssetId)
    }

    public struct PoolAssetPair {
        public let asset1: PoolAsset
        public let asset2: PoolAsset

        public init(asset1: PoolAsset, asset2: PoolAsset) {
            self.asset1 = asset1
            self.asset2 = asset2
        }
    }

    public struct AssetIdPair: JSONListConvertible {
        public let asset1: AssetId
        public let asset2: AssetId

        public init(jsonList: [JSON], context: [CodingUserInfoKey: Any]?) throws {
            let expectedFieldsCount = 1
            let actualFieldsCount = jsonList.count
            guard expectedFieldsCount == actualFieldsCount else {
                throw JSONListConvertibleError.unexpectedNumberOfItems(
                    expected: expectedFieldsCount,
                    actual: actualFieldsCount
                )
            }

            guard let poolId = jsonList[0].arrayValue, poolId.count == 2 else {
                throw JSONListConvertibleError.unexpectedValue(jsonList[0])
            }

            asset1 = try poolId[0].map(to: AssetId.self, with: context)
            asset2 = try poolId[1].map(to: AssetId.self, with: context)
        }
    }
}

public extension AssetConversionPallet.AssetId {
    var location: XcmUni.RelativeLocation {
        wrapped.location
    }

    init(parents: UInt8, interior: XcmUni.Junctions) {
        self.init(
            wrapped: XcmUni.AssetId(
                location: XcmUni.RelativeLocation(
                    parents: parents,
                    interior: interior
                )
            )
        )
    }
}

public protocol AssetConversionAssetIdProtocol {
    var parents: UInt8 { get }
    var items: [XcmUni.Junction] { get }
}

extension AssetConversionPallet.AssetId: AssetConversionAssetIdProtocol {
    public var parents: UInt8 { location.parents }
    public var items: [XcmUni.Junction] { location.interior.items }
}

extension XcmUni.RelativeLocation: AssetConversionAssetIdProtocol {
    public var items: [XcmUni.Junction] { interior.items }
}
