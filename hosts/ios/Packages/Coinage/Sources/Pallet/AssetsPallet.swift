import Foundation
import SubstrateSdk
import BigInt

/// Minimal local model of pallet-assets — only the per-account balance the incoming-payment claim
/// tracks. A fuller model lives in the AssetsManagement package, but Coinage must not depend on that
/// package's graph, so this decodes just what `AssetsTracking` needs.
enum AssetsPallet {
    static let name = "Assets"

    enum Storage {
        /// `Assets.Account(assetId, accountId) -> AssetAccount` — an account's balance in an asset.
        static func account() -> StorageCodingPath {
            StorageCodingPath(moduleName: AssetsPallet.name, itemName: "Account")
        }
    }

    /// The subset of `pallet_assets::AssetAccount` the tracker reads.
    struct Account: Decodable {
        @StringCodable var balance: BigUInt
    }
}
