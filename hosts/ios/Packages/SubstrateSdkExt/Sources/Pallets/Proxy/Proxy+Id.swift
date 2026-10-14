import Foundation

extension Proxy.ProxyType {
    init(id: String) {
        switch id {
        case "any":
            self = .any
        case "nonTransfer":
            self = .nonTransfer
        case "governance":
            self = .governance
        case "staking":
            self = .staking
        case "nominationPools":
            self = .nominationPools
        case "identityJudgement":
            self = .identityJudgement
        case "cancelProxy":
            self = .cancelProxy
        case "auction":
            self = .auction
        default:
            self = .other(id)
        }
    }

    var id: String {
        switch self {
        case .any:
            "any"
        case .nonTransfer:
            "nonTransfer"
        case .governance:
            "governance"
        case .staking:
            "staking"
        case .nominationPools:
            "nominationPools"
        case .identityJudgement:
            "identityJudgement"
        case .cancelProxy:
            "cancelProxy"
        case .auction:
            "auction"
        case let .other(value):
            value
        }
    }
}
