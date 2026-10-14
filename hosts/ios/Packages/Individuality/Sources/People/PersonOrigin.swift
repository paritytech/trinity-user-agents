import Foundation
import KeyDerivation

public enum PersonOrigin {
    case lite(MembersPallet.RingIndex, BandersnatchKeyManaging)
    case full(MembersPallet.RingIndex, BandersnatchKeyManaging)

    public var ringIndex: MembersPallet.RingIndex {
        switch self {
        case let .lite(ringIndex, _):
            ringIndex
        case let .full(ringIndex, _):
            ringIndex
        }
    }

    public var keyManager: BandersnatchKeyManaging {
        switch self {
        case let .lite(_, keyManager):
            keyManager
        case let .full(_, keyManager):
            keyManager
        }
    }
}

public extension PersonOrigin {
    var collectionIdentifier: MembersPallet.CollectionIdentifier {
        switch self {
        case .lite: PeopleLitePallet.membersIdentifier
        case .full: PeoplePallet.membersIdentifier
        }
    }
}
