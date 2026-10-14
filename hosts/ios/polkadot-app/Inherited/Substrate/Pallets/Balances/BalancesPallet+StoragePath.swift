import Foundation
import SubstrateSdk

extension BalancesPallet {
    static var holdsPath: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "Holds")
    }

    static var freezesPath: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "Freezes")
    }
}
