import Foundation
import SubstrateSdk

public extension ResourcesPallet {
    static var statementStoreAllowances: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "StatementStoreAllowances")
    }

    static var spentLongTermStorageAliases: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "SpentLongTermStorageAliases")
    }
}
