import Foundation
import SubstrateSdk

public extension PGASPallet {
    /// Aliases that have been used to claim PGAS, keyed by (day, alias).
    /// Day uses big-endian encoding with Identity hashing; alias uses Blake2_128Concat.
    static var claimedGasAliases: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "ClaimedGasAliases")
    }
}
