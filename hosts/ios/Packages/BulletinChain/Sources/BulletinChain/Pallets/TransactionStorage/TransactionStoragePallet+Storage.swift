import Foundation
import SubstrateSdk

public extension TransactionStoragePallet {
    static var authorizationsPath: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "Authorizations")
    }
}
