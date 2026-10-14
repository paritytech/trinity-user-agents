import Foundation

struct IdentityProfile: Equatable {
    enum Rank: Equatable {
        case basic
        case membership
    }

    let username: Username?
    let isClaimed: Bool
    let rank: Rank
}
