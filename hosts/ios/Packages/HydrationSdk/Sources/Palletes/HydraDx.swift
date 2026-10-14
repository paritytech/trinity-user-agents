import Foundation
import BigInt
import SubstrateSdk

public enum HydraDx {
    public typealias AssetId = BigUInt
    public static let nativeAssetId = AssetId(0)
    public static let dynamicFeesModule = "DynamicFees"
    public static let multiTxPaymentModule = "MultiTransactionPayment"
    public static let referralsModule = "Referrals"

    public struct AssetsKey: JSONListConvertible {
        public let assetId: HydraDx.AssetId

        public init(jsonList: [JSON], context: [CodingUserInfoKey: Any]?) throws {
            guard jsonList.count == 1 else {
                throw JSONListConvertibleError.unexpectedNumberOfItems(
                    expected: 1,
                    actual: jsonList.count
                )
            }

            assetId = try jsonList[0].map(
                to: StringScaleMapper<HydraDx.AssetId>.self,
                with: context
            ).value
        }
    }

    public struct FeeParameters: Decodable {
        @StringCodable public var minFee: BigUInt
    }

    public struct FeeEntry: Decodable {
        @StringCodable public var assetFee: BigUInt
        @StringCodable public var protocolFee: BigUInt
    }
}

public extension HydraDx {
    struct LocalRemoteAssetId: Equatable, Hashable {
        public let localAssetId: ChainAssetId
        public let remoteAssetId: HydraDx.AssetId

        public init(localAssetId: ChainAssetId, remoteAssetId: HydraDx.AssetId) {
            self.localAssetId = localAssetId
            self.remoteAssetId = remoteAssetId
        }
    }

    struct SwapPair: Equatable, Hashable {
        public let assetIn: LocalRemoteAssetId
        public let assetOut: LocalRemoteAssetId

        public init(assetIn: LocalRemoteAssetId, assetOut: LocalRemoteAssetId) {
            self.assetIn = assetIn
            self.assetOut = assetOut
        }
    }

    struct LocalSwapPair: Equatable, Hashable {
        public let assetIn: ChainAssetId
        public let assetOut: ChainAssetId

        public init(assetIn: ChainAssetId, assetOut: ChainAssetId) {
            self.assetIn = assetIn
            self.assetOut = assetOut
        }
    }

    struct RemoteSwapPair: Equatable, Hashable {
        public let assetIn: HydraDx.AssetId
        public let assetOut: HydraDx.AssetId

        public init(assetIn: HydraDx.AssetId, assetOut: HydraDx.AssetId) {
            self.assetIn = assetIn
            self.assetOut = assetOut
        }
    }
}
