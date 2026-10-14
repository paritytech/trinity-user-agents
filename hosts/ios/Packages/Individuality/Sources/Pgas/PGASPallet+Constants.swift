import Foundation
import SubstrateSdkExt

public extension PGASPallet {
    enum Constants {
        case maxClaimsPerPeriodPerPerson
        case maxClaimsPerPeriodPerLitePerson
        case pgasClaimAmount
    }
}

extension PGASPallet.Constants: ConstantPathConvertible {
    public var name: String {
        switch self {
        case .maxClaimsPerPeriodPerPerson:
            "MaxClaimsPerPeriodPerPerson"
        case .maxClaimsPerPeriodPerLitePerson:
            "MaxClaimsPerPeriodPerLitePerson"
        case .pgasClaimAmount:
            "PgasClaimAmount"
        }
    }

    public var moduleName: String { PGASPallet.name }
}
