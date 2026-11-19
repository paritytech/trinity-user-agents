import Foundation
import SubstrateSdkExt

/// Namespace for the dotNS gateway pallet's storage paths and stored types.
public enum DotnsGatewayPallet {
    /// Runtime module name the pallet is registered under.
    public static let name = "DotnsGateway"
}

public extension DotnsGatewayPallet {
    enum Constants {
        case signingContext
    }
}

extension DotnsGatewayPallet.Constants: ConstantPathConvertible {
    public var name: String {
        switch self {
        case .signingContext:
            "SigningContext"
        }
    }

    public var moduleName: String { DotnsGatewayPallet.name }
}
