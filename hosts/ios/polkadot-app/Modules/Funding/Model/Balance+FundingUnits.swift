import BigInt
import Foundation
import SubstrateSdk
import TrUAPIHost

extension Balance {
    /// An amount as the core sends it: the decimal string of the asset's
    /// smallest unit. Nil when there is none or it is not a number.
    init?(units: U128?) {
        guard let units, let balance = Balance(units) else { return nil }
        self = balance
    }

    /// The amount as the core takes it.
    var units: U128 {
        String(self)
    }
}
