import Foundation
import SubstrateSdk

extension HydraAssetRegistry {
    static var assetsPath: StorageCodingPath {
        StorageCodingPath(moduleName: module, itemName: "Assets")
    }
}
