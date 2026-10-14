import Foundation

public enum StakingType: String, Codable, Equatable, Hashable {
    case relaychain
    case parachain
    case azero = "aleph-zero"
    case auraRelaychain = "aura-relaychain"
    case turing
    case nominationPools = "nomination-pools"
    case unsupported

    public init(rawType: String?) {
        if let rawType, let value = StakingType(rawValue: rawType) {
            self = value
        } else {
            self = .unsupported
        }
    }

    public func isMorePreferred(than stakingType: StakingType) -> Bool {
        StakingClass(stakingType: self).preferringRating < StakingClass(stakingType: stakingType).preferringRating
    }
}

public enum StakingClass {
    case relaychain
    case parachain
    case nominationPools
    case unsupported

    // lesser better
    public var preferringRating: UInt8 {
        switch self {
        case .relaychain:
            0
        case .parachain:
            1
        case .nominationPools:
            2
        case .unsupported:
            3
        }
    }

    public init(stakingType: StakingType) {
        switch stakingType {
        case .relaychain,
             .azero,
             .auraRelaychain:
            self = .relaychain
        case .parachain,
             .turing:
            self = .parachain
        case .nominationPools:
            self = .nominationPools
        case .unsupported:
            self = .unsupported
        }
    }
}
