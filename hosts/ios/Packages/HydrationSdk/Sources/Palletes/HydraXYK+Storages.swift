import Foundation
import SubstrateSdk

extension HydraXYK {
    static var poolAssetsPath: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "PoolAssets")
    }

    static var exchangeFeePath: ConstantCodingPath {
        ConstantCodingPath(moduleName: name, constantName: "GetExchangeFee")
    }
}
