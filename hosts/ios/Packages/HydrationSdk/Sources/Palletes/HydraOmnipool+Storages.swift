import Foundation
import SubstrateSdk

extension HydraOmnipool {
    static var hubAssetIdPath: ConstantCodingPath {
        ConstantCodingPath(moduleName: moduleName, constantName: "HubAssetId")
    }

    static var assetsPath: StorageCodingPath {
        StorageCodingPath(moduleName: moduleName, itemName: "Assets")
    }
}
