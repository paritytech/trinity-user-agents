import Foundation
import SubstrateSdkExt

public extension PeopleLitePallet {
    enum Constants {
        case signingContext
    }
}

extension PeopleLitePallet.Constants: ConstantPathConvertible {
    public var name: String {
        switch self {
        case .signingContext:
            "SigningContext"
        }
    }

    public var moduleName: String { PeopleLitePallet.name }
}
