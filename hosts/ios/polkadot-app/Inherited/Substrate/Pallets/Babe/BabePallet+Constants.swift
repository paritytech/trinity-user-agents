import Foundation
import SubstrateSdk
import SubstrateSdkExt

extension BabePallet {
    enum Constants {
        case sessionLength
        case babeBlockTime
    }
}

extension BabePallet.Constants: ConstantPathConvertible {
    var moduleName: String {
        BabePallet.name
    }

    var name: String {
        switch self {
        case .sessionLength:
            "EpochDuration"
        case .babeBlockTime:
            "ExpectedBlockTime"
        }
    }
}
