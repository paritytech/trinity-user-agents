import Foundation
import SubstrateSdk

extension ParaId? {
    var isSystemParachain: Bool {
        switch self {
        case .none:
            false
        case let .some(paraId):
            paraId.isSystemParachain
        }
    }

    var isRelay: Bool {
        switch self {
        case .none:
            true
        case .some:
            false
        }
    }

    var isRelayOrSystemParachain: Bool {
        switch self {
        case .none:
            true
        case let .some(paraId):
            paraId.isSystemParachain
        }
    }
}

extension ParaId {
    var isSystemParachain: Bool {
        self >= 1_000 && self < 2_000
    }
}
