import Foundation
import SubstrateSdk

public extension AssetsPallet {
    static func accountPath(from moduleName: String?) -> StorageCodingPath {
        StorageCodingPath(moduleName: moduleName ?? "Assets", itemName: "Account")
    }

    static func detailsPath(from moduleName: String?) -> StorageCodingPath {
        StorageCodingPath(moduleName: moduleName ?? "Assets", itemName: "Asset")
    }
}
