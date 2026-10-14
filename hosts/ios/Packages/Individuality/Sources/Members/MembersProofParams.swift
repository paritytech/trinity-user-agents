import Foundation
import BandersnatchApi

public struct MembersProofParams {
    public let ringMembers: [MembersPallet.RingMember]
    public let ringSize: BandersnatchApi.RingDomainSize
    public let revision: MembersPallet.RevisionIndex

    public init(
        ringMembers: [MembersPallet.RingMember],
        ringSize: BandersnatchApi.RingDomainSize,
        revision: MembersPallet.RevisionIndex
    ) {
        self.ringMembers = ringMembers
        self.ringSize = ringSize
        self.revision = revision
    }
}
