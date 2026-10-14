import Foundation
import SubstrateSdkExt

public extension MembersPallet {
    enum Constants {
        case maxFlexibleRingExponent
    }
}

extension MembersPallet.Constants: ConstantPathConvertible {
    public var name: String {
        switch self {
        case .maxFlexibleRingExponent:
            "MaxFlexibleRingExponent"
        }
    }

    public var moduleName: String { MembersPallet.name }
}
