import Foundation
import SubstrateSdk

public extension PeoplePallet {
    static var accountToAliasPath: StorageCodingPath {
        .init(moduleName: name, itemName: "AccountToAlias")
    }
}
