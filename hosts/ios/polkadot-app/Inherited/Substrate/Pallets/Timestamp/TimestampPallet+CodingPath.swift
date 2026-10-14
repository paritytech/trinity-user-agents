import Foundation
import SubstrateSdk

extension TimestampPallet {
    static var minimumPeriodBetweenBlocksPath: ConstantCodingPath {
        ConstantCodingPath(moduleName: name, constantName: "MinimumPeriod")
    }

    static var timestampNowPath: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "Now")
    }
}
