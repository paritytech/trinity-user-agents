import Foundation
import SubstrateSdk

extension HydraStableswap {
    static var pools: StorageCodingPath {
        StorageCodingPath(moduleName: module, itemName: "Pools")
    }

    static var tradability: StorageCodingPath {
        StorageCodingPath(moduleName: module, itemName: "AssetTradability")
    }

    static var poolPegs: StorageCodingPath {
        StorageCodingPath(moduleName: module, itemName: "PoolPegs")
    }
}
