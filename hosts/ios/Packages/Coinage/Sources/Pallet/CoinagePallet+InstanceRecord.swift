import Foundation
import SubstrateSdk
import BigInt

extension CoinagePallet {
    /// Per-instance configuration from `Instances`. Only the fields the app reads are modelled;
    /// `asset_unit` replaced the removed `UnderlyingAssetUnit` pallet constant in v0.12.0.
    struct InstanceRecord: Decodable {
        @StringCodable var assetUnit: BigUInt
    }
}
